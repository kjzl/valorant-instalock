use std::fmt::Display;

use anyhow::{bail, Context};
use serde::{Deserialize, Serialize};

pub async fn agent_from_name(agent_name: &str) -> anyhow::Result<GameAgent> {
    crate::global::GAME_AGENTS
        .get()
        .unwrap()
        .iter()
        .find(|a| a.name.0.eq(agent_name))
        .cloned()
        .context(format!("Agent '{agent_name}' could not be found."))
}

pub async fn map_from_map_url(map_url: &str) -> anyhow::Result<GameMap> {
    crate::global::GAME_MAPS
        .get()
        .unwrap()
        .iter()
        .find(|m| m.map_url.0.eq(map_url))
        .cloned()
        .context(format!("Map '{map_url}' could not be found."))
}

pub async fn fetch_api_version() -> anyhow::Result<ValorantApiVersion> {
    let api_version_response = reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(2))
        .build()?
        .get("https://valorant-api.com/v1/version")
        .send()
        .await?
        .text()
        .await?;

    let version: ValorantApiVersionResponse =
        serde_json::from_str(&api_version_response).inspect_err(|_| {
            log::debug!("api_version_response: {:#?}", api_version_response);
        })?;
    if !http::StatusCode::from_u16(version.status)?.is_success() {
        bail!(
            "failed to fetch version ({}: {}), response:\n{:#?}",
            version.status,
            http::StatusCode::from_u16(version.status)?
                .canonical_reason()
                .unwrap_or_else(|| "unknown response code"),
            version
        )
    }
    Ok(version.data)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameAgent {
    pub uuid: String,
    pub name: AgentName,
}

impl GameAgent {
    pub async fn fetch_all() -> anyhow::Result<Vec<GameAgent>> {
        let fetch_all_agents_response = reqwest::ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(2))
            .build()?
            .get("https://valorant-api.com/v1/agents")
            .send()
            .await?
            .text()
            .await?;

        let mut agents: ValorantApiAgentResponse = serde_json::from_str(
            &fetch_all_agents_response,
        )
        .inspect_err(|_| {
            log::debug!(
                "fetch_all_agents_response: {:#?}",
                fetch_all_agents_response
            );
        })?;
        if !http::StatusCode::from_u16(agents.status)?.is_success() {
            bail!(
                "failed to fetch agents ({}: {}), response:\n{:#?}",
                agents.status,
                http::StatusCode::from_u16(agents.status)?
                    .canonical_reason()
                    .unwrap_or_else(|| "unknown response code"),
                agents
            )
        }
        agents.data.sort_by_cached_key(|k| k.display_name.clone());
        Ok(agents.data.into_iter().map(GameAgent::from).collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameMap {
    pub uuid: String,
    pub name: MapName,
    pub map_url: MapUrl,
}

impl GameMap {
    pub async fn fetch_all() -> anyhow::Result<Vec<GameMap>> {
        let fetch_all_maps_response = reqwest::ClientBuilder::new()
            .timeout(std::time::Duration::from_secs(2))
            .build()?
            .get("https://valorant-api.com/v1/maps")
            .send()
            .await?
            .text()
            .await?;

        let mut maps: ValorantApiMapResponse = serde_json::from_str(
            &fetch_all_maps_response,
        )
        .inspect_err(|_| {
            log::debug!(
                "fetch_all_maps_response: {:#?}",
                fetch_all_maps_response
            );
        })?;
        if !http::StatusCode::from_u16(maps.status)?.is_success() {
            bail!(
                "failed to fetch maps ({}: {}), response:\n{:#?}",
                maps.status,
                http::StatusCode::from_u16(maps.status)?
                    .canonical_reason()
                    .unwrap_or_else(|| "unknown response code"),
                maps
            )
        }
        maps.data.sort_by_cached_key(|k| k.display_name.clone());
        Ok(maps.data.into_iter().map(GameMap::from).collect())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentName(pub String);

impl Display for AgentName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapName(pub String);

impl Display for MapName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapUrl(pub String);

impl Display for MapUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Display for GameMap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl From<ValorantApiMap> for GameMap {
    fn from(value: ValorantApiMap) -> Self {
        GameMap {
            uuid: value.uuid,
            name: MapName(value.display_name),
            map_url: MapUrl(value.map_url),
        }
    }
}

impl Display for GameAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl From<ValorantApiAgent> for GameAgent {
    fn from(value: ValorantApiAgent) -> Self {
        GameAgent {
            uuid: value.uuid,
            name: AgentName(value.display_name),
        }
    }
}

pub type Url = String;
pub type ValorantApiAgentResponse = ValorantApiResponse<Vec<ValorantApiAgent>>;
pub type ValorantApiMapResponse = ValorantApiResponse<Vec<ValorantApiMap>>;
pub type ValorantApiVersionResponse = ValorantApiResponse<ValorantApiVersion>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValorantApiResponse<T> {
    status: u16,
    data: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValorantApiAgent {
    uuid: String,
    display_name: String,
    // whether the agent is a playable character (there are two sova's)
    is_playable_character: bool,
    // small square icon
    display_icon_small: Option<Url>,
    // full agent portrait
    full_portrait_v2: Option<Url>,
    // wide killfeed icon
    killfeed_portrait: Option<Url>,
    // and more... see response of https://valorant-api.com/v1/agents
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValorantApiMap {
    uuid: String,
    display_name: String,
    map_url: String,
    // large map image
    splash: Option<Url>,
    // large minimap image
    display_icon: Option<Url>,
    // 456x100 map icon
    list_view_icon: Option<Url>,
    // and more... see response of https://valorant-api.com/v1/maps
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValorantApiVersion {
    // 4223B9537F74423A
    pub manifest_id: String,
    // release-08.05
    pub branch: String,
    // 08.05.00.2367061
    pub version: String,
    // 9
    pub build_version: String,
    // 4.27.2.0
    pub engine_version: String,
    // release-08.05-shipping-9-2367061
    pub riot_client_version: String,
    // 82.0.3.1237.2870
    pub riot_client_build: String,
    // 2024-03-15T00:00:00Z
    pub build_date: chrono::DateTime<chrono::Local>,
}

impl Default for ValorantApiVersion {
    fn default() -> Self {
        Self {
            manifest_id: "unknown manifest_id".into(),
            branch: "unknown branch".into(),
            version: "unknown version".into(),
            build_version: "unknown build_version".into(),
            engine_version: "unknown engine_version".into(),
            riot_client_version: "unknown riot_client_version".into(),
            riot_client_build: "unknown riot_client_build".into(),
            build_date: Default::default(),
        }
    }
}

impl Display for ValorantApiVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        static FMT: std::sync::LazyLock<chrono::format::StrftimeItems> =
            std::sync::LazyLock::new(|| {
                chrono::format::StrftimeItems::new_with_locale(
                    "%d.%m.%Y %H:%M",
                    *crate::locale::SYS_LOCALE,
                )
            });
        write!(
            f,
            "Valorant API Version: {} ({})",
            self.riot_client_version,
            self.build_date.format_localized_with_items(
                FMT.clone(),
                *crate::locale::SYS_LOCALE,
            )
        )
    }
}
