use anyhow::Context;
use parking_lot::lock_api::ArcMutexGuard;
use parking_lot::{Mutex, RawMutex};
use reqwest::Client;
use std::ops::Deref;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::{channel, Sender};
use tokio::time::sleep_until;
use tokio::time::Instant;

use self::stream::ValorantEventStream;
use self::types::ValorantClientAuth;
use crate::global::{API_VERSION, GAME_MAPS};
use crate::valorant_client::http::ProductId;
use crate::valorant_client::types::ClientStatus;
use crate::valorant_client::types::GameLoopState;
use crate::INTERRUPT;
use crate::{
    config::Config, lockfile::Lockfile, valorant_client::stream::ValorantEvent,
};

mod http;
mod stream;
mod types;

pub enum ValorantCommand {
    QuitPregame,
    QuitGame,
}

/// Drop to stop the client
#[derive(Debug, Clone)]
pub struct ValorantClientHandle {
    tx: Sender<ValorantCommand>,
}

#[derive(Debug, Clone)]
pub struct ShardRegion {
    shard: String,
    region: String,
}

pub enum MaybeValorantClient {
    Client(ValorantClient),
    Parts(Lockfile, Config),
}

impl MaybeValorantClient {
    pub async fn init(lockfile: Lockfile, config: Config) -> Self {
        match ValorantClient::init(lockfile.clone(), config.clone()).await {
            Ok(client) => Self::Client(client),
            Err(err) => {
                log::error!("Failed to initialize client, trying to init another time later: {}", err);
                Self::Parts(lockfile, config)
            }
        }
    }

    pub async fn retry_init(&mut self) {
        let parts = match self {
            Self::Parts(lockfile, config) => (lockfile, config),
            _ => return,
        };
        match ValorantClient::init(parts.0.clone(), parts.1.clone()).await {
            Ok(client) => {
                *self = Self::Client(client);
            }
            Err(err) => {
                log::error!("Failed to initialize client: {}", err);
            }
        }
    }

    pub fn client(&self) -> Option<ValorantClient> {
        match self {
            Self::Client(client) => Some(client.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ValorantClient {
    client: Client,
    pub running: Arc<AtomicBool>,
    pub config: Config,
    pub lockfile: Lockfile,
    pub shard: String,
    pub region: String,
    pub subject: String,
    pub version: String,
    pub platform: String,
    auth: Arc<Mutex<ValorantClientAuth>>,
    current_match_id: Arc<Mutex<Option<String>>>,
    loop_state: Arc<Mutex<GameLoopState>>,
}

impl ValorantClient {
    pub async fn init(
        lockfile: Lockfile,
        config: Config,
    ) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .timeout(std::time::Duration::from_millis(1500))
            .build()
            .unwrap();
        let auth = Self::fetch_auth_tokens(&client, &lockfile).await?;
        let session = Self::sessions_info(&client, &lockfile)
            .await?
            .into_iter()
            .find_map(|(_, session)| {
                if session.product_id == ProductId::Valorant {
                    Some(session)
                } else {
                    None
                }
            })
            .context(
                "No Valorant session returned by local sessions endpoint",
            )?;
        let region = session
            .launch_configuration
            .region()
            .context("No region found in session launchargs")?;
        let shard = session
            .launch_configuration
            .shard()
            .context("No shard found in session launchargs")?;
        let subject = auth.subject.clone();
        //let version = session.version;
        let version = API_VERSION.get().unwrap().riot_client_version.clone();
        let platform = "ew0KCSJwbGF0Zm9ybVR5cGUiOiAiUEMiLA0KCSJwbGF0Zm9ybU9TIjogIldpbmRvd3MiLA0KCSJwbGF0Zm9ybU9TVmVyc2lvbiI6ICIxMC4wLjE5MDQyLjEuMjU2LjY0Yml0IiwNCgkicGxhdGZvcm1DaGlwc2V0IjogIlVua25vd24iDQp9".to_string();
        // let pregame = client.current_pregame(&auth, shard_region, puuid)
        // lock agent
        let this = Self::new(
            client, subject, config, region, shard, version, platform, auth,
            lockfile,
        );
        match this.current_pregame().await {
            Ok(pregame) => {
                this.set_loop_state(GameLoopState::Pregame);
                let _ = this.current_match_id.lock().replace(pregame.match_id);
                let _ = this.handle_pregame(false).await;
            }
            Err(err) => {
                log::error!("Failed to fetch pregame match: {}", err);
                let ingame = this.current_ingame().await;
                if let Ok(ingame) = ingame {
                    this.set_loop_state(GameLoopState::Ingame);
                    log::info!("In Game: {}", ingame.match_id);
                    *this.current_match_id() = Some(ingame.match_id);
                }
            }
        }
        Ok(this)
    }

    pub fn new(
        client: Client,
        subject: String,
        config: Config,
        region: String,
        shard: String,
        version: String,
        platform: String,
        auth: ValorantClientAuth,
        lockfile: Lockfile,
    ) -> Self {
        Self {
            client,
            config,
            running: Arc::new(AtomicBool::new(true)),
            auth: Arc::new(Mutex::new(auth)),
            region,
            shard,
            subject,
            version,
            platform,
            lockfile,
            current_match_id: Arc::new(Mutex::new(None)),
            loop_state: Arc::new(Mutex::new(GameLoopState::Menus)),
        }
    }

    pub fn auth(&self) -> ArcMutexGuard<RawMutex, ValorantClientAuth> {
        self.auth.lock_arc()
    }

    pub fn current_match_id(&self) -> ArcMutexGuard<RawMutex, Option<String>> {
        self.current_match_id.lock_arc()
    }

    pub fn loop_state(&self) -> GameLoopState {
        *self.loop_state.lock()
    }

    pub fn set_loop_state(&self, loop_state: GameLoopState) {
        *self.loop_state.lock() = loop_state;
    }

    async fn handle_pregame(&self, wait: bool) -> Option<()> {
        let begin_event = Instant::now();
        let instalock_wait = sleep_until(
            begin_event + Duration::from_millis(self.config.instalock_wait_ms),
        );
        log::info!(
            "handle pregame (Pregame started): {}",
            self.current_match_id().deref().as_ref()?
        );
        if INTERRUPT.load(std::sync::atomic::Ordering::Relaxed) {
            log::info!("Interrupted.");
            return None;
        }
        let map = match self.get_pregame_match().await {
            Ok(pregame) => GAME_MAPS
                .get()
                .unwrap()
                .iter()
                .find(|map| map.map_url.0 == pregame.map_url)
                .unwrap(),
            Err(err) => {
                eprintln!("Failed to fetch pregame match map: {}", err);
                eprintln!("Proceeding with Ascent as map.");

                log::error!("Failed to fetch pregame match map: {}", err);
                GAME_MAPS
                    .get()
                    .unwrap()
                    .iter()
                    .find(|map| map.name.0 == "Ascent")
                    .unwrap()
            }
        };
        let now = chrono::Local::now();
        eprintln!(
            "{} - Entered Pregame for {}",
            now.format("%H:%M:%S"),
            console::style(format!("{}", map.name.0)).cyan()
        );
        let agents = self.config.get_agents(map.name.0.as_str());
        let mut i = 0;
        // initial wait
        if wait {
            instalock_wait.await;
            log::info!(
                "Instalock wait finished ({}ms)",
                self.config.instalock_wait_ms
            );
        }
        while i < agents.len()
            && self.lock_agent(agents[i].uuid.as_str()).await.is_err()
        {
            log::error!("Failed to lock agent {}", &agents[i].name);
            i += 1;
        }
        if i < agents.len() {
            let failed_attempts = if i > 0 {
                format!(" after {} failed attempts", i)
            } else {
                "".to_string()
            };
            let now = chrono::Local::now();
            eprintln!(
                "{} - Instalocked {} after {}ms{failed_attempts}",
                now.format("%H:%M:%S"),
                console::style(format!("{}", agents[i].name)).cyan(),
                tokio::time::Instant::now()
                    .duration_since(begin_event)
                    .as_millis(),
            );
            log::info!("Locked agent: {}", &agents[i].name);
        }
        Some(())
    }
}

impl ValorantClientHandle {
    fn spawn_cmd_handler(
        mut cmd_rx: Receiver<ValorantCommand>,
        client_state: Arc<Mutex<MaybeValorantClient>>,
    ) {
        tokio::task::spawn(async move {
            let mut client = None;
            loop {
                if client.is_none() {
                    client_state.lock().retry_init().await;
                    let Some(unwrapped) = client_state.lock().client() else {
                        continue;
                    };
                    client = Some(unwrapped);
                }
                let client = client.as_ref().unwrap();
                let Some(cmd) = cmd_rx.recv().await else {
                    log::info!(
                        "Command channel was closed. Shutting down Client."
                    );
                    client
                        .running
                        .store(false, std::sync::atomic::Ordering::Relaxed);
                    break;
                };
                match cmd {
                    ValorantCommand::QuitPregame => {
                        log::info!("Quitting pregame");
                        match client.quit_pregame().await {
                            Ok(_) => log::info!("Pregame quit successfully"),
                            Err(err) => {
                                log::error!("Failed to quit pregame: {}", err)
                            }
                        }
                    }
                    ValorantCommand::QuitGame => {
                        log::info!("Quitting game");
                        match client.quit_ingame().await {
                            Ok(_) => log::info!("Game quit successfully"),
                            Err(err) => {
                                log::error!("Failed to quit game: {}", err)
                            }
                        }
                    }
                }
            }
        });
    }

    async fn init_client(client_state: &ValorantClient, lockfile: &Lockfile) {
        /*match http_client.fetch_auth_tokens(&lockfile).await {
            Ok(auth) => client_state.set_client_auth(auth),
            Err(err) => {
                log::error!("Failed to fetch auth tokens: {}", err);
                return;
            }
        };*/
        /*match http_client
            .current_player(client_state.client_auth().as_ref().unwrap())
            .await
        {
            Ok(player) => {
                client_state.set_current_subject(player.subject);
            }
            Err(err) => {
                log::error!("Failed to fetch player info: {}", err);
                return;
            }
        };*/
        /*match http_client
            .current_pregame(
                client_state.client_auth().as_ref().unwrap(),
                &client_state.shard_region(),
                client_state.current_subject().as_ref().unwrap(),
            )
            .await
        {
            Ok(pregame) => {
                client_state.set_loop_state(GameLoopState::Pregame);
                client_state.set_current_match_id(pregame.match_id);
            }
            Err(err) => {
                log::error!("Failed to fetch pregame match: {}", err);
                let ingame = http_client
                    .current_ingame(
                        client_state.client_auth().as_ref().unwrap(),
                        &client_state.shard_region(),
                        client_state.current_subject().as_ref().unwrap(),
                    )
                    .await;
                if let Ok(ingame) = ingame {
                    client_state.set_loop_state(GameLoopState::Ingame);
                    log::info!("In Game: {}", ingame.match_id);
                    client_state.set_current_match_id(ingame.match_id.clone());
                }
                return;
            }
        };*/
    }

    fn spawn_event_handler(
        mut client_state: Arc<Mutex<MaybeValorantClient>>,
        mut stream: ValorantEventStream,
    ) {
        tokio::task::spawn(async move {
            //Self::init_client(&http_client, &client_state, &lockfile).await;
            //log::info!(
            //    "Client initialized, now trying to handle pregame if necessary"
            //);
            //Self::handle_pregame(false, &config, &client_state, &http_client)
            //    .await;
            let mut client = None;
            loop {
                let Some(event) = stream.next().await else {
                    log::info!("Event stream ended. Shutting down Client.");
                    break;
                };
                if client.is_none() {
                    client_state.lock().retry_init().await;
                    let Some(unwrapped) = client_state.lock().client() else {
                        continue;
                    };
                    client = Some(unwrapped);
                }
                let client = client.as_ref().unwrap();
                if !client.running.load(std::sync::atomic::Ordering::Relaxed) {
                    log::info!("Client was dropped. Shutting down.");
                    break;
                }

                match event {
                    ValorantEvent::EntitlementsTokenChanged(auth) => {
                        *client.auth() = auth;
                    }
                    ValorantEvent::ClientInfo(ClientStatus {
                        subject,
                        loop_state: GameLoopState::Pregame,
                        maybe_match_id: match_id,
                    }) => {
                        if client.loop_state() == GameLoopState::Pregame {
                            continue;
                        }
                        client.set_loop_state(GameLoopState::Pregame);
                        *client.current_match_id() = Some(match_id);
                        let _ = client.handle_pregame(true).await;
                    }
                    ValorantEvent::ClientInfo(ClientStatus {
                        subject,
                        loop_state: GameLoopState::Ingame,
                        maybe_match_id: match_id,
                    }) => {
                        if client.loop_state() == GameLoopState::Ingame {
                            continue;
                        }
                        let now = chrono::Local::now();
                        eprintln!("{} - Match started", now.format("%H:%M:%S"));
                        log::info!("Match started: {match_id}");
                        client.set_loop_state(GameLoopState::Ingame);
                        *client.current_match_id() = Some(match_id);
                    }
                    ValorantEvent::ClientInfo(ClientStatus {
                        subject,
                        loop_state: GameLoopState::Menus,
                        ..
                    }) => {
                        if client.loop_state() == GameLoopState::Menus {
                            continue;
                        }
                        let now = chrono::Local::now();
                        eprintln!("{} - Match ended", now.format("%H:%M:%S"));
                        log::info!("Pregame/Match ended");
                        client.set_loop_state(GameLoopState::Menus);
                        *client.current_match_id() = None;
                    }
                }
            }
        });
    }

    pub async fn start(
        lockfile: Lockfile,
        config: Config,
    ) -> anyhow::Result<Self> {
        let stream = ValorantEventStream::connect(&lockfile).await?;
        let (cmd_tx, cmd_rx) = channel(100);
        let client_state = Arc::new(Mutex::new(
            MaybeValorantClient::init(lockfile, config).await,
        ));
        Self::spawn_cmd_handler(cmd_rx, Arc::clone(&client_state));
        Self::spawn_event_handler(client_state, stream);
        Ok(Self { tx: cmd_tx })
    }

    // This does only wait for the command to be sent to the client
    // It does not wait for the command to be finished executing
    pub async fn quit_pregame(&self) {
        self.tx.send(ValorantCommand::QuitPregame).await.unwrap();
    }

    pub async fn quit_game(&self) {
        self.tx.send(ValorantCommand::QuitGame).await.unwrap();
    }
}
