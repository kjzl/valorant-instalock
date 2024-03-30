#![feature(lazy_cell)]
#![feature(fs_try_exists)]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::bail;
use anyhow::Context;
use anyhow::Result;
use config::Config;
use crossterm::event;
use crossterm::event::KeyCode;
use crossterm::event::KeyEventKind;
use dialoguer::console::style;
use dialoguer::theme::ColorfulTheme;
use directories::ProjectDirs;
use futures::FutureExt;
use futures::StreamExt;
use indicatif::ProgressBar;

use crate::global::API_VERSION;
use crate::global::GAME_AGENTS;
use crate::global::GAME_MAPS;
use crate::lockfile::watch_lockfile;
use crate::valorant_client::ValorantClient;

mod config;
mod global;
mod locale;
mod lockfile;
mod logging;
mod valo_types;
mod valorant_client;

/// VALORANT API DOCS: https://valapidocs.techchrism.me/endpoint/sessions

pub mod built_info {
    // The file has been placed there by the build script.
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

pub static PROJECT_DIRS: LazyLock<ProjectDirs> = LazyLock::new(|| {
    ProjectDirs::from("", "kjzl", "valorant-instalock")
        .context("Could not find the app directory")
        .unwrap()
});

pub static LOG_DIR: LazyLock<PathBuf> =
    LazyLock::new(|| PROJECT_DIRS.data_dir().join("logs"));

pub static CACHE_FILES: LazyLock<CacheFiles> = LazyLock::new(|| CacheFiles {
    agents: PROJECT_DIRS.cache_dir().join("agents.json"),
    maps: PROJECT_DIRS.cache_dir().join("maps.json"),
    api_version: PROJECT_DIRS.cache_dir().join("api_version.json"),
});

pub static CONFIG_FILES: LazyLock<ConfigFiles> =
    LazyLock::new(|| ConfigFiles {
        version: PROJECT_DIRS.config_dir().join("version.json"),
        config: PROJECT_DIRS.config_dir().join("config_v1.json"),
    });

pub static DIALOG_THEME: LazyLock<ColorfulTheme> =
    LazyLock::new(|| ColorfulTheme::default());

pub static INTERRUPT: AtomicBool = AtomicBool::new(false);

pub static DONT_SAVE_CONFIG: AtomicBool = AtomicBool::new(false);

pub static CONFIG: tokio::sync::OnceCell<Config> =
    tokio::sync::OnceCell::const_new();

pub struct CacheFiles {
    pub agents: PathBuf,
    pub maps: PathBuf,
    pub api_version: PathBuf,
}

pub struct ConfigFiles {
    pub version: PathBuf,
    pub config: PathBuf,
}

async fn handle_major_version_change(v: anyhow::Result<String>) {
    match v {
        Ok(v) => log::warn!(
            "Used a different major version before: {v} ==> {}",
            built_info::PKG_VERSION_MAJOR
        ),
        Err(err) => log::warn!("Failed to load previous version: {err}"),
    }
    log::info!("Purging potentially outdated cache files...");
    let _ = tokio::join!(
        tokio::fs::remove_file(&CACHE_FILES.agents),
        tokio::fs::remove_file(&CACHE_FILES.maps),
        tokio::fs::remove_file(&CACHE_FILES.api_version)
    );
}

fn init_config() -> anyhow::Result<Config> {
    Ok(match Config::read() {
        Ok(ok) => ok,
        Err(err) => {
            eprintln!("{}", style("Failed to read config file!").red());
            log::warn!("Failed to read config file: {err}");
            if dialoguer::Confirm::with_theme(&*DIALOG_THEME)
                .with_prompt(
                    "Do you want to proceed with a new temporary config?",
                )
                .interact()
                .unwrap()
            {
                DONT_SAVE_CONFIG
                    .store(true, std::sync::atomic::Ordering::Relaxed);
                Config::default()
            } else {
                bail!("User chose not to proceed with temporary config")
            }
        }
    })
}

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let dbg_build = if built_info::PROFILE.eq("debug") {
        " (DEBUG BUILD)"
    } else {
        ""
    };
    println!(
        "valorant-instalock v{}{} made by {}\n",
        built_info::PKG_VERSION,
        dbg_build,
        built_info::PKG_AUTHORS
    );
    logging::init_logging();
    let _ = std::fs::create_dir_all(PROJECT_DIRS.cache_dir());
    let _ = std::fs::create_dir_all(PROJECT_DIRS.config_dir());
    match tokio::fs::read_to_string(&CONFIG_FILES.version).await {
        Err(err) => {
            handle_major_version_change(
                Err(err).context("Failed to read previous version from file"),
            )
            .await;
        }
        Ok(v) if v.ne(built_info::PKG_VERSION_MAJOR) => {
            handle_major_version_change(Ok(v)).await;
        }
        // version equals current version
        Ok(_) => (),
    }
    CONFIG.set(init_config()?).unwrap();

    let progress = ProgressBar::new_spinner();
    progress.enable_steady_tick(Duration::from_millis(75));
    global::init_globals(progress.clone()).await;
    progress.println(format!("{}", API_VERSION.get().unwrap()));
    progress.finish();
    let mut lockfile_watcher = watch_lockfile().await?;
    let valorant_client: Arc<Mutex<Option<ValorantClient>>> =
        Arc::new(Mutex::new(None));
    let menu_valorant_client = Arc::clone(&valorant_client);
    // TODO: FIXME when opening program when game already running, check if player is in pregame already!!!!!
    // https://valapidocs.techchrism.me/endpoint/pre-game-player to get pregame match id
    //
    // add menu option for Dodge Game
    // add menu option for Dodge Pregame

    // TODO: Add ability to open log file via menu entry
    eprintln!("For menu options or to interrupt/pause the application, press shift + tab in the console window");
    let interrupt_task = tokio::task::spawn(async move {
        let mut stream = event::EventStream::new();
        while let Some(event) = stream.next().fuse().await {
            match event {
                //Ok(event::Event::Key(event)) => eprintln!("{:#?}", event),
                Ok(event::Event::Key(key_event))
                    if (key_event.code == KeyCode::BackTab
                        || key_event.code == KeyCode::Tab)
                        && key_event.kind == KeyEventKind::Press
                        && key_event
                            .modifiers
                            .intersects(event::KeyModifiers::SHIFT) =>
                {
                    eprintln!("Application Interrupted/Paused by shift + tab");
                    log::warn!(
                        "Interrupted by shift + tab at {}",
                        chrono::Local::now()
                    );
                    INTERRUPT.store(true, std::sync::atomic::Ordering::Relaxed);
                    let items = [
                        "Quit Pregame (Dodge)",
                        "Quit Ingame",
                        "Change Config",
                        "Open Log Folder",
                    ];
                    if let Some(i) =
                        dialoguer::Select::with_theme(&*DIALOG_THEME)
                            .items(&items)
                            .interact_opt()
                            .unwrap()
                    {
                        if i == 0 {
                            log::info!("Selected Quit Pregame in Menu");
                            let send_client =
                                menu_valorant_client.lock().unwrap().clone();
                            if let Some(client) = send_client {
                                client.quit_pregame().await;
                            } else {
                                log::warn!("No ValorantClient available to quit pregame");
                            }
                        } else if i == 1 {
                            log::info!("Selected Quit Ingame in Menu");
                            let send_client =
                                menu_valorant_client.lock().unwrap().clone();
                            if let Some(client) = send_client {
                                client.quit_game().await;
                            } else {
                                log::warn!("No ValorantClient available to quit ingame");
                            }
                        } else if i == 2 {
                            let items =
                                ["Edit agents", "Edit initial instalock delay"];
                            let i =
                                dialoguer::Select::with_theme(&*DIALOG_THEME)
                                    .items(&items)
                                    .interact_opt()
                                    .unwrap();
                            if i == Some(0) {
                                if let Some(cfg) = Config::prompt_map_agent_cfg(
                                    Some(CONFIG.get().unwrap().clone()),
                                    GAME_MAPS.get().unwrap(),
                                    GAME_AGENTS.get().unwrap(),
                                ) {
                                    cfg.write().unwrap();
                                    eprintln!("New config:");
                                    eprintln!("{}", cfg.map_agent_config);
                                    eprintln!("");
                                    eprintln!("{}", style("Changes will be applied after restarting the application.").yellow());
                                }
                            } else if i == Some(1) {
                                let cfg = Config::prompt_instalock_wait_ms(
                                    Some(CONFIG.get().unwrap().clone()),
                                );
                                cfg.write().unwrap();
                                eprintln!(
                                    "New initial Instalock delay: {}ms",
                                    cfg.instalock_wait_ms
                                );
                                eprintln!("");
                                eprintln!("{}", style("Changes will be applied after restarting the application.").yellow());
                            }
                        } else if i == 3 {
                            if let Err(err) = open::that_detached(&*LOG_DIR) {
                                eprintln!("Failed to open log folder: {err}");
                                log::error!("Failed to open log folder: {err}");
                            }
                        }
                    }
                    INTERRUPT
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    eprintln!("Application Resumed");
                    log::warn!(
                        "Resuming from Interrupt at {}",
                        chrono::Local::now()
                    );
                }
                Err(err) => log::warn!("Error in console event stream: {err}"),
                _ => (),
            }
        }
    });

    loop {
        match lockfile_watcher.recv().await {
            Some(lockfile::LockfileEvent::Created(lockfile)) => {
                log::info!("Lockfile created/modified: {lockfile:?}",);
                log::info!("Starting ValorantClient");
                *valorant_client.lock().unwrap() = Some(
                    match ValorantClient::start(
                        lockfile,
                        CONFIG.get().unwrap().clone(),
                    )
                    .await
                    {
                        Ok(ok) => ok,
                        Err(err) => {
                            log::error!(
                                "Failed to start ValorantClient: {err}"
                            );
                            continue;
                        }
                    },
                );
            }
            Some(lockfile::LockfileEvent::Deleted) => {
                log::info!("Lockfile deleted",);
                *valorant_client.lock().unwrap() = None;
            }
            None => {
                log::info!("Lockfile event channel was closed");
                break;
            }
        }
    }

    //let _ = tokio::join!(interrupt_task);

    if console::user_attended_stderr() {
        eprintln!("");
        eprintln!("Press Enter to exit...");
        std::io::stdin().read_line(&mut String::new()).unwrap();
    }
    Ok(())
}

// #[tokio::main]
// async fn main() -> Result<()> {
//     let _log_handle = log::init_log4rs();
//     let mut http_client = WeakHttpClient::new();
//     let (resources, cfg) = init_resources_and_load_config(&http_client).await?;

//     let app = AppState::new(cfg, resources).await?;
//     http_client.init_weak_app_state(Arc::downgrade(&app));
//     let http_client = http_client.with_app_state().unwrap();
//     let temp_app = Arc::clone(&app);
//     let http_client_2 = http_client.clone();

//     let command_task = tokio::task::spawn_blocking(move || {
//         let mut theme = ColorfulTheme::default();

//         // https://github.com/console-rs/dialoguer/issues/202
//         theme.unchecked_item_prefix =
//             style("X".to_string()).for_stderr().black();

//         let resources = temp_app.resources.as_ref().unwrap();

//         let (agents, maps) = (resources.agents.clone(), resources.maps.clone());

//         let mut prompt = dialoguer::Select::with_theme(&theme);
//         let items: Vec<&str> = vec![
//             "Quit PreGame (Dodge)",
//             "Change Config and reload agents & maps data",
//         ];
//         prompt.items(&items);
//         loop {
//             let select = prompt
//                 // .with_prompt("")
//                 .interact()
//                 .unwrap();
//             let http_client_3 = http_client_2.clone();
//             match select {
//                 0 => {
//                     let pregame_match_id = match temp_app.gamestate() {
//                         ShooterGameState::Pregame(id) => id,
//                         _ => {
//                             println!("not in pregame");
//                             continue;
//                         }
//                     };
//                     tokio::task::spawn(async move {
//                         http_client_3.quit_pregame(&pregame_match_id).await
//                     });
//                 }
//                 1 => {
//                     // tokio::task::spawn(async move {
//                     //     fs::remove_file(Config::config_path().unwrap()).unwrap();
//                     //     let (resources, cfg) =
//                     //         init_resources_and_load_config(&WeakHttpClient::new())
//                     //             .await
//                     //             .unwrap();
//                     // });
//                 }
//                 _ => unimplemented!(),
//             }
//         }
//     });

//     let task: anyhow::Result<()> = tokio::task::spawn(async move {
//         let mut ws: Option<LocalWebSocket> = None;

//         while let Some(event) = app.recv_app_event().await {
//             let state = Arc::clone(&app);
//             if let AppEvent::ShooterGameState(game_state) = &event {
//                 state.set_gamestate(game_state.clone());
//             }
//             match event {
//                 // try close websocket
//                 // might also be called right before riot client is being started
//                 AppEvent::ShooterGameState(ShooterGameState::Offline) => {
//                     info!("ShooterGameState::Offline");
//                     state.set_lockfile(None);

//                     if let Some(ws) = ws.take() {
//                         if let Err(_err) = ws.close().await {
//                             // ignore error
//                             // warn!("error closing the websocket: {_err}");
//                         }
//                     }
//                 }
//                 // open websocket
//                 // might be called twice upon starting Riot Client, we do not want two websocket connections open
//                 AppEvent::ShooterGameState(ShooterGameState::Online) => {
//                     if ws.is_none() {
//                         info!("ShooterGameState::Online");
//                     state.set_lockfile(Some(
//                         fs::read_to_string(&state.lockfile_path())
//                             .unwrap()
//                             .parse()
//                             .unwrap(),
//                     ));

//                     let event_kinds = vec![
//                         WS_JSON_EVENT_MESSAGING_SERVICE.to_owned(),
//                         WS_JSON_EVENT_ENTITLEMENTS_TOKEN.to_owned(), // TODO maybe we can listen for when entitlements token changes...
//                     ];
//                     match LocalWebSocket::connect_and_listen(state, event_kinds).await {
//                         Ok(_ws) => {
//                             ws.replace(_ws);
//                         }
//                         Err(err) => {
//                             warn!("websocket error: {err}");
//                         }
//                     }
//                     }
//                 }
//                 // instalock agent
//                 // set app_state Pregame for context in commands
//                 AppEvent::ShooterGameState(ShooterGameState::Pregame(match_id)) => {
//                     info!("ShooterGameState::Pregame({match_id})");
//                     match instalock(&state, &http_client, &match_id).await {
//                         Some((agent, map)) => {
//                             println!("Instalocked {agent} on {map}");
//                             info!("Instalocked {agent} on {map}");
//                         }
//                         None => {
//                             warn!("Did not instalock for current match (Probably Config or Game Resources not available): {match_id}");
//                         }
//                     }
//                 }
//                 // set app_state other for context in commands
//                 AppEvent::ShooterGameState(ShooterGameState::Other(phase)) => {
//                     info!("ShooterGameState::Other({phase})")
//                 }
//                 AppEvent::AuthTokensJson(json) => {
//                     info!("AuthTokensJson(_)");
//                     if let Err(err) = http_client.parse_auth_tokens(&json) {
//                         warn!("{err}");
//                     }
//                 }
//                 AppEvent::Error(err) => warn!("{err}"),
//                 AppEvent::ManualEditConfig | AppEvent::ManualDodgeMatch | AppEvent::ManualLockAgent => (),
//             }
//         }
//         Ok(())
//     })
//     .await?;

//     Ok(())
// }

// // TODO maybe redo this
// async fn instalock(
//     state: &Arc<AppState>,
//     http_client: &AppStateHttpClient,
//     match_id: &str,
// ) -> Option<(GameAgent, GameMap)> {
//     let pregame_match = http_client.pregame_match(&match_id).await;
//     let res = match pregame_match {
//         Ok(pregame) => Some(pregame),
//         Err(err) => {
//             warn!(
//                 "Pregame match request error (instalocking cancelled): {err}"
//             );
//             None
//         }
//     }?;
//     let pregame_json: serde_json::Map<String, Value> =
//         serde_json::from_str(&res.text().await.unwrap()).unwrap();

//     let map = state.resources.as_ref()?.map_with_url(
//         &pregame_json
//             .get("MapID")
//             .unwrap()
//             .as_str()
//             .unwrap()
//             .to_owned(),
//     )?;
//     let agent = state
//         .resources
//         .as_ref()?
//         .agent_with_name(&state.cfg.as_ref()?.get_agent(&map.name)?)?;

//     let ms = 500;
//     info!("Now waiting {ms}ms until we try to instalock {agent} on {map}.");
//     tokio::time::sleep(Duration::from_millis(ms)).await;

//     let agent_lock = http_client.lock_agent(&match_id, &agent.uuid).await;
//     match agent_lock {
//         Ok(res) => Some(res),
//         Err(err) => {
//             warn!("Instalock request error (instalocking cancelled): {err}");
//             None
//         }
//     }?;

//     Some((agent.clone(), map.clone()))
// }

// let _reyna = "a3bfb853-43b2-7238-a4f1-ad90e9e46bcc";
//             let _jett = "add6443a-41bd-e414-f6ad-e58d267f4e95";
//             let agent = _jett; // hardcode agent
//             let ms = 5000u64;
//             info!("waiting {ms}");
//             tokio::time::sleep(Duration::from_millis(ms)).await;
//             info!("about to lock agent");
//
