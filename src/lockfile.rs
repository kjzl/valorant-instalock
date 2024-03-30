use std::path::Path;

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::{
    fs,
    sync::mpsc::{channel, Receiver},
};

#[derive(Debug, Clone)]
pub struct Lockfile {
    pub name: String,
    pub pid: i128,
    pub port: u32,
    pub password: String,
    pub protocol: String,
}

impl Lockfile {
    pub fn parse(file: &str) -> Option<Self> {
        let mut v: Vec<&str> = file.split(':').collect();
        Some(Lockfile {
            protocol: v.pop()?.into(),
            password: v.pop()?.into(),
            port: v.pop()?.parse().ok()?,
            pid: v.pop()?.parse().ok()?,
            name: v.pop()?.into(),
        })
    }

    pub fn websocket_addr(&self) -> String {
        format!("wss://127.0.0.1:{}/", self.port)
    }

    pub fn http_addr(&self) -> String {
        format!("https://127.0.0.1:{}/", self.port)
    }

    pub fn auth(&self) -> String {
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            format!("riot:{}", &self.password),
        )
    }

    pub fn auth_header(&self) -> http::HeaderValue {
        http::HeaderValue::from_str(&format!("Basic {}", self.auth())).unwrap()
    }
}

pub enum LockfileEvent {
    Created(Lockfile),
    Deleted,
}

async fn init_lockfile_watcher(
    lockfile: &Path,
) -> anyhow::Result<(
    Receiver<Result<notify::Event, notify::Error>>,
    RecommendedWatcher,
)> {
    let (watcher_tx, watcher_rx) = channel(100);
    // initial check for lockfile - if it exists, imitate a watcher event we listen to
    if fs::try_exists(&lockfile).await.is_ok_and(|exists| exists) {
        let modify_event = notify::Event::new(EventKind::Modify(
            notify::event::ModifyKind::Any,
        ))
        .add_path(lockfile.to_owned());
        watcher_tx.send(Ok(modify_event)).await.unwrap();
    }
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = watcher_tx.blocking_send(event);
        },
        notify::Config::default(),
    )?;
    watcher.watch(lockfile.parent().unwrap(), RecursiveMode::NonRecursive)?;
    Ok((watcher_rx, watcher))
}

pub async fn watch_lockfile() -> anyhow::Result<Receiver<LockfileEvent>> {
    let lockfile = directories::BaseDirs::new()
        .unwrap()
        .data_local_dir()
        .join(r#"Riot Games\Riot Client\Config\lockfile"#);
    let (mut watcher_rx, watcher) = init_lockfile_watcher(&lockfile).await?;
    let (tx, rx) = channel(10);
    tokio::task::spawn(async move {
        #[allow(unused)]
        let watcher = watcher;
        loop {
            let msg = match watcher_rx.recv().await {
                Some(ok) => ok,
                None => {
                    eprintln!(
                        "Underlying Lockfile watcher stopped unexpectedly."
                    );
                    Err(notify::Error::generic("watcher stopped unexpectedly"))
                }
            };
            match &msg {
                Ok(notify::Event {
                    kind: EventKind::Modify(_),
                    paths,
                    ..
                }) => {
                    if paths.iter().any(|path| path.as_path().eq(&lockfile)) {
                        // Riot Client or Valorant is running
                        let lockfile_str = match fs::read_to_string(&lockfile)
                            .await
                        {
                            Ok(ok) => ok,
                            Err(err) => {
                                eprintln!("There was an error reading the Valorant lockfile. Can't start listening to Valorant events.");
                                log::error!("could not read lockfile {err}");
                                continue;
                            }
                        };
                        let parsed = match Lockfile::parse(&lockfile_str) {
                            Some(ok) => ok,
                            None => {
                                log::error!("lockfile parse error, lockfile: {lockfile_str}");
                                continue;
                            }
                        };
                        if let Err(_) =
                            tx.send(LockfileEvent::Created(parsed)).await
                        {
                            log::info!("lockfile event channel was closed");
                            break;
                        }
                    }
                }
                Ok(notify::Event {
                    kind: EventKind::Remove(_),
                    paths,
                    ..
                }) => {
                    if paths.iter().any(|path| path.as_path().eq(&lockfile)) {
                        // Valorant was stopped
                        if let Err(_) = tx.send(LockfileEvent::Deleted).await {
                            log::info!("lockfile event channel was closed");
                            break;
                        }
                    }
                }
                Ok(_) => log::trace!("ignoring lockfile notify event {msg:#?}"),
                Err(err) => {
                    log::warn!("lockfile watcher error {err}");
                }
            };
        }
    });
    Ok(rx)
}
