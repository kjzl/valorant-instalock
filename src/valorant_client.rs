use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::{channel, Sender};
use tokio::time::sleep_until;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::client;

use self::http::ValorantHttpClient;
use self::stream::ValorantEventStream;
use self::types::ValorantClientAuth;
use crate::global::GAME_MAPS;
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
pub struct ValorantClient {
    tx: Sender<ValorantCommand>,
}

#[derive(Debug, Clone)]
pub struct ShardRegion {
    shard: String,
    region: String,
}

#[derive(Debug, Clone)]
pub struct ValorantClientState {
	pub running: Arc<AtomicBool>,
    client_auth: Arc<Mutex<Option<ValorantClientAuth>>>,
    shard_region: Arc<Mutex<ShardRegion>>,
    current_subject: Arc<Mutex<Option<String>>>,
    current_match_id: Arc<Mutex<Option<String>>>,
    loop_state: Arc<Mutex<GameLoopState>>,
}

impl ValorantClientState {
    pub fn new() -> Self {
        Self {
			running: Arc::new(AtomicBool::new(true)),
            client_auth: Arc::new(Mutex::new(None)),
            shard_region: Arc::new(Mutex::new(ShardRegion {
                shard: "eu".into(),
                region: "eu".into(),
            })),
            current_subject: Arc::new(Mutex::new(None)),
            current_match_id: Arc::new(Mutex::new(None)),
            loop_state: Arc::new(Mutex::new(GameLoopState::Menus)),
        }
    }

    pub fn client_auth(&self) -> Option<ValorantClientAuth> {
        self.client_auth.lock().unwrap().clone()
    }

    pub fn set_client_auth(&self, auth: ValorantClientAuth) {
        *self.client_auth.lock().unwrap() = Some(auth);
    }

    pub fn unset_client_auth(&self) {
        *self.client_auth.lock().unwrap() = None;
    }

    pub fn shard_region(&self) -> ShardRegion {
        self.shard_region.lock().unwrap().clone()
    }

    pub fn set_shard_region(&self, shard_region: ShardRegion) {
        *self.shard_region.lock().unwrap() = shard_region;
    }

    pub fn current_subject(&self) -> Option<String> {
        self.current_subject.lock().unwrap().clone()
    }

    pub fn set_current_subject(&self, subject: String) {
        *self.current_subject.lock().unwrap() = Some(subject);
    }

    pub fn unset_current_subject(&self) {
        *self.current_subject.lock().unwrap() = None;
    }

    pub fn current_match_id(&self) -> Option<String> {
        self.current_match_id.lock().unwrap().clone()
    }

    pub fn set_current_match_id(&self, match_id: String) {
        *self.current_match_id.lock().unwrap() = Some(match_id);
    }

    pub fn unset_current_match_id(&self) {
        *self.current_match_id.lock().unwrap() = None;
    }

    pub fn loop_state(&self) -> GameLoopState {
        *self.loop_state.lock().unwrap()
    }

    pub fn set_loop_state(&self, loop_state: GameLoopState) {
        *self.loop_state.lock().unwrap() = loop_state;
    }
}

impl ValorantClient {
    fn spawn_cmd_handler(
        mut cmd_rx: Receiver<ValorantCommand>,
        client_state: ValorantClientState,
    ) {
        tokio::task::spawn(async move {
            let http_client = ValorantHttpClient::new();
            loop {
                let Some(cmd) = cmd_rx.recv().await else {
                    log::info!(
                        "Command channel was closed. Shutting down Client."
                    );
					client_state.running.store(false, std::sync::atomic::Ordering::Relaxed);
                    break;
                };
                match cmd {
                    ValorantCommand::QuitPregame => {
                        log::info!("Quitting pregame");
                        let Some(ref client_auth) = client_state.client_auth()
                        else {
                            log::error!("No client auth tokens available. Cannot quit pregame.");
                            continue;
                        };
                        let Some(ref current_match_id) =
                            client_state.current_match_id()
                        else {
                            log::error!("No current match id available. Cannot quit pregame.");
                            continue;
                        };
                        match http_client
                            .quit_pregame(
                                &client_auth,
                                &client_state.shard_region(),
                                &current_match_id,
                            )
                            .await
                        {
                            Ok(_) => log::info!("Pregame quit successfully"),
                            Err(err) => {
                                log::error!("Failed to quit pregame: {}", err)
                            }
                        }
                    }
                    ValorantCommand::QuitGame => {
                        log::info!("Quitting game");
                        let Some(ref client_auth) = client_state.client_auth()
                        else {
                            log::error!("No client auth tokens available. Cannot quit game.");
                            continue;
                        };
                        let Some(ref current_match_id) =
                            client_state.current_match_id()
                        else {
                            log::error!("No current match id available. Cannot quit game.");
                            continue;
                        };
                        let Some(ref current_subject) =
                            client_state.current_subject()
                        else {
                            log::error!("No current subject available. Cannot quit game.");
                            continue;
                        };
                        match http_client
                            .quit_ingame(
                                &client_auth,
                                &client_state.shard_region(),
                                &current_subject,
                                &current_match_id,
                            )
                            .await
                        {
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

    async fn handle_pregame(
        wait: bool,
        config: &Config,
        client_state: &ValorantClientState,
        http_client: &ValorantHttpClient,
    ) {
        let begin_event = Instant::now();
        let instalock_wait = sleep_until(
            begin_event + Duration::from_millis(config.instalock_wait_ms),
        );
        let Some(match_id) = client_state.current_match_id() else {
            log::error!("No match id available. Cannot lock agent.");
            return;
        };
        log::info!("handle pregame (Pregame started): {match_id}");
        if INTERRUPT.load(std::sync::atomic::Ordering::Relaxed) {
            log::info!("Interrupted.");
            return;
        }
        let Some(client_auth) = client_state.client_auth() else {
            log::error!("No client auth tokens available. Cannot lock agent.");
            return;
        };
        // TODO if there happen to be issues with invalid auth tokens, just fetch new everytime we before we lock an agent
        let map_url = match http_client
            .get_pregame_match(
                &client_auth,
                &client_state.shard_region(),
                &match_id,
            )
            .await
        {
            Ok(pregame) => pregame.map_url,
            Err(err) => {
                log::error!("Failed to fetch pregame match map: {}", err);
                return;
            }
        };
        let map = GAME_MAPS
            .get()
            .unwrap()
            .iter()
            .find(|map| map.map_url.0 == map_url)
            .unwrap();
        let now = chrono::Local::now();
        eprintln!(
            "{} - Entered Pregame for {}",
            now.format("%H:%M:%S"),
            console::style(format!("{}", map.name.0)).cyan()
        );
        let agents = config.get_agents(map.name.0.as_str());
        let mut i = 0;
        // initial wait
        if wait {
            instalock_wait.await;
            log::info!(
                "Instalock wait finished ({}ms)",
                config.instalock_wait_ms
            );
        }
        while i < agents.len()
            && http_client
                .lock_agent(
                    &client_auth,
                    &client_state.shard_region(),
                    &match_id,
                    agents[i].uuid.as_str(),
                )
                .await
                .is_err()
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
    }

    async fn init_client(
        http_client: &ValorantHttpClient,
        client_state: &ValorantClientState,
        lockfile: &Lockfile,
    ) {
        match http_client.fetch_auth_tokens(&lockfile).await {
            Ok(auth) => client_state.set_client_auth(auth),
            Err(err) => {
                log::error!("Failed to fetch auth tokens: {}", err);
                return;
            }
        };
        match http_client
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
        };
        match http_client
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
        };
    }

    fn spawn_event_handler(
        client_state: ValorantClientState,
        lockfile: Lockfile,
        config: Config,
        mut stream: ValorantEventStream,
    ) {
        tokio::task::spawn(async move {
            let http_client = ValorantHttpClient::new();
            // TODO properly fetch shard and region
            Self::init_client(&http_client, &client_state, &lockfile).await;
            log::info!(
                "Client initialized, now trying to handle pregame if necessary"
            );
            Self::handle_pregame(false, &config, &client_state, &http_client)
                .await;
            loop {
				if !client_state.running.load(std::sync::atomic::Ordering::Relaxed) {
					log::info!("Client was dropped. Shutting down.");
					break;
				}
                let Some(event) = stream.next().await else {
                    log::info!("Event stream ended. Shutting down Client.");
                    break;
                };
                match event {
                    ValorantEvent::EntitlementsTokenChanged(auth) => {
                        client_state.set_client_auth(auth);
                    }
                    ValorantEvent::ClientInfo(ClientStatus {
                        subject,
                        loop_state: GameLoopState::Pregame,
                        maybe_match_id: match_id,
                    }) => {
                        if client_state.loop_state() == GameLoopState::Pregame {
                            continue;
                        }
                        client_state.set_loop_state(GameLoopState::Pregame);
                        client_state.set_current_subject(subject);
                        client_state.set_current_match_id(match_id.clone());
                        Self::handle_pregame(
                            true,
                            &config,
                            &client_state,
                            &http_client,
                        )
                        .await;
                    }
                    ValorantEvent::ClientInfo(ClientStatus {
                        subject,
                        loop_state: GameLoopState::Ingame,
                        maybe_match_id: match_id,
                    }) => {
                        if client_state.loop_state() == GameLoopState::Ingame {
                            continue;
                        }
                        let now = chrono::Local::now();
                        eprintln!("{} - Match started", now.format("%H:%M:%S"));
                        log::info!("Match started: {match_id}");
                        client_state.set_loop_state(GameLoopState::Ingame);
                        client_state.set_current_subject(subject);
                        client_state.set_current_match_id(match_id.clone());
                    }
                    ValorantEvent::ClientInfo(ClientStatus {
                        subject,
                        loop_state: GameLoopState::Menus,
                        ..
                    }) => {
                        if client_state.loop_state() == GameLoopState::Menus {
                            continue;
                        }
                        let now = chrono::Local::now();
                        eprintln!("{} - Match ended", now.format("%H:%M:%S"));
                        log::info!("Pregame/Match ended");
                        client_state.set_loop_state(GameLoopState::Menus);
                        client_state.set_current_subject(subject);
                        client_state.unset_current_match_id();
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
        let client_state = ValorantClientState::new();
        Self::spawn_cmd_handler(cmd_rx, client_state.clone());
        Self::spawn_event_handler(client_state, lockfile, config, stream);
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
