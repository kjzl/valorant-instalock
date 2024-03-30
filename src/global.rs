use console::style;
use indicatif::ProgressBar;
use tokio::sync::OnceCell;

use crate::{
    valo_types::{fetch_api_version, GameAgent, GameMap, ValorantApiVersion},
    CACHE_FILES,
};

pub static API_VERSION: OnceCell<ValorantApiVersion> = OnceCell::const_new();
pub static GAME_MAPS: OnceCell<Vec<GameMap>> = OnceCell::const_new();
pub static GAME_AGENTS: OnceCell<Vec<GameAgent>> = OnceCell::const_new();

async fn init_from_remote(
) -> anyhow::Result<(ValorantApiVersion, Vec<GameAgent>, Vec<GameMap>)> {
    let (api_version, agents, maps) = tokio::join!(
        fetch_api_version(),
        GameAgent::fetch_all(),
        GameMap::fetch_all(),
    );
    log::debug!("init_from_remote");
    log::debug!("api_version: {:#?}", api_version);
    log::debug!("agents: {:#?}", agents);
    log::debug!("maps: {:#?}", maps);
    Ok((api_version?, agents?, maps?))
}

async fn init_from_cache(
) -> anyhow::Result<(ValorantApiVersion, Vec<GameAgent>, Vec<GameMap>)> {
    let (api_version, agents, maps) = tokio::join!(
        tokio::fs::read(&CACHE_FILES.api_version),
        tokio::fs::read(&CACHE_FILES.agents),
        tokio::fs::read(&CACHE_FILES.maps),
    );
    log::debug!("init_from_cache");
    log::debug!("FILES:");
    log::debug!("api_version: {:#?}", api_version);
    log::debug!("agents: {:#?}", agents);
    log::debug!("maps: {:#?}", maps);

    let api_version = serde_json::from_slice(&api_version?);
    let agents = serde_json::from_slice(&agents?);
    let maps = serde_json::from_slice(&maps?);
    log::debug!("PARSED:");
    log::debug!("api_version: {:#?}", api_version);
    log::debug!("agents: {:#?}", agents);
    log::debug!("maps: {:#?}", maps);
    Ok((api_version?, agents?, maps?))
}

pub async fn init_globals(progress: ProgressBar) {
    let (api_version, agents, maps) = match init_from_remote().await {
        Ok(ok) => ok,
        Err(err) => {
            log::warn!("Failed to fetch Valorant API data: {err}");
            log::warn!("Attempting to load from cache...");
            progress.println(format!(
                "{}, trying to proceed anyways... (see logs for error)",
                style("Failed to fetch Maps & Agents from Valorant API").red()
            ));
            match init_from_cache().await {
                Ok(ok) => ok,
                Err(err) => {
                    log::warn!(
                        "Failed to load Valorant API data from cache: {err}"
                    );
                    log::warn!("Proceeding without API data...");
                    (ValorantApiVersion::default(), vec![], vec![])
                }
            }
        }
    };
    API_VERSION.set(api_version).unwrap();
    GAME_AGENTS.set(agents).unwrap();
    GAME_MAPS.set(maps).unwrap();
}
