use crate::lockfile::Lockfile;
use anyhow::Result;
use reqwest::{Client, RequestBuilder, Response};
use serde::Deserialize;

use super::{types::ValorantClientAuth, ShardRegion};

const RIOT_ENTITLEMENTS_HEADER: &str = "X-Riot-Entitlements-JWT";

pub struct ValorantHttpClient {
    client: Client,
}

impl ValorantHttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .danger_accept_invalid_certs(true)
                .timeout(std::time::Duration::from_millis(1500))
                .build()
                .unwrap(),
        }
    }

    pub async fn fetch_auth_tokens(
        &self,
        lockfile: &Lockfile,
    ) -> Result<ValorantClientAuth> {
        log::debug!("Sending auth tokens request. lockfile: {:#?}", lockfile);
        let res = send_with_retry(with_local_auth(
            self.client
                .get(format!("{}entitlements/v1/token", lockfile.http_addr())),
            lockfile,
        ))
        .await?
        .error_for_status()?
        .json()
        .await
        .map_err(Into::into);
        log::debug!("fetch auth tokens response: {:#?}", res);
        res
    }

    pub async fn quit_pregame(
        &self,
        auth: &ValorantClientAuth,
        shard_region: &ShardRegion,
        match_id: &str,
    ) -> Result<()> {
        log::debug!("Sending quit pregame match request: {match_id}");
        let (shard, region) = (&shard_region.shard, &shard_region.region);
        let res = send_with_retry(with_remote_auth(self.client.post(format!("https://glz-{region}-1.{shard}.a.pvp.net/pregame/v1/matches/{match_id}/quit")), auth)).await?.error_for_status()?;
        log::debug!("quit pregame response: {res:#?}");
        log::debug!("quit pregame response body: {:#?}", res.text().await);
        Ok(())
    }

    pub async fn lock_agent(
        &self,
        auth: &ValorantClientAuth,
        shard_region: &ShardRegion,
        match_id: &str,
        agent_id: &str,
    ) -> Result<()> {
        log::debug!("Sending lock agent request: {agent_id}, {match_id}");
        let (shard, region) = (&shard_region.shard, &shard_region.region);
        let res = send_with_retry(with_remote_auth(self.client.post(format!("https://glz-{region}-1.{shard}.a.pvp.net/pregame/v1/matches/{match_id}/lock/{agent_id}")), auth)).await?.error_for_status()?;
        log::debug!("lock agent response: {res:#?}");
        log::debug!("lock agent response body: {:#?}", res.text().await);
        Ok(())
    }

    // https://auth.riotgames.com/userinfo
    pub async fn current_player(
        &self,
        auth: &ValorantClientAuth,
    ) -> Result<PlayerInfo> {
        log::debug!("Sending current player request");
        let res = send_with_retry(
            self.client
                .get("https://auth.riotgames.com/userinfo")
                .bearer_auth(&auth.access_token),
        )
        .await?
        .error_for_status()?;
        log::debug!("current player response: {res:#?}");
        let res = res.json().await;
        log::debug!("current player response body: {res:#?}");
        res.map_err(Into::into)
    }

    pub async fn get_pregame_match(
        &self,
        auth: &ValorantClientAuth,
        shard_region: &ShardRegion,
        match_id: &str,
    ) -> Result<PregameMatch> {
        log::debug!("Sending get pregame match request: {match_id}");
        let (shard, region) = (&shard_region.shard, &shard_region.region);
        let res = send_with_retry(with_remote_auth(self.client.get(format!("https://glz-{region}-1.{shard}.a.pvp.net/pregame/v1/matches/{match_id}")), auth)).await?.error_for_status()?;
        log::debug!("get pregame match response: {res:#?}");
        let res = res.text().await;
        log::debug!("get pregame match response body: {res:#?}");
        serde_json::from_str(&res?).map_err(Into::into)
    }

    //https://glz-{region}-1.{shard}.a.pvp.net/pregame/v1/players/{puuid}
    pub async fn current_pregame(
        &self,
        auth: &ValorantClientAuth,
        shard_region: &ShardRegion,
        puuid: &str,
    ) -> Result<CurrentPlayerPregame> {
        log::debug!("Sending current pregame match request: {puuid}");
        let (shard, region) = (&shard_region.shard, &shard_region.region);
        let res = send_with_retry(with_remote_auth(self.client.get(format!("https://glz-{region}-1.{shard}.a.pvp.net/pregame/v1/players/{puuid}")), auth)).await?.error_for_status()?;
        log::debug!("current pregame response: {res:#?}");
        let res = res.text().await;
        log::debug!("current pregame response body: {res:#?}");
        serde_json::from_str(&res?).map_err(Into::into)
    }

    //https://glz-{region}-1.{shard}.a.pvp.net/core-game/v1/players/{puuid}/disassociate/{current game match id}
    pub async fn quit_ingame(
        &self,
        auth: &ValorantClientAuth,
        shard_region: &ShardRegion,
        puuid: &str,
        match_id: &str,
    ) -> Result<()> {
        log::debug!("Sending quit ingame match request: {match_id}");
        let (shard, region) = (&shard_region.shard, &shard_region.region);
        let res = send_with_retry(with_remote_auth(self.client.post(format!("https://glz-{region}-1.{shard}.a.pvp.net/core-game/v1/players/{puuid}/disassociate/{match_id}")), auth)).await?.error_for_status()?;
        log::debug!("quit ingame response: {res:#?}");
        log::debug!("quit ingame response body: {:#?}", res.text().await);
        Ok(())
    }

    //https://glz-{region}-1.{shard}.a.pvp.net/core-game/v1/players/{puuid}
    pub async fn current_ingame(
        &self,
        auth: &ValorantClientAuth,
        shard_region: &ShardRegion,
        puuid: &str,
    ) -> Result<CurrentPlayerIngame> {
        log::debug!("Sending current ingame match request: {puuid}");
        let (shard, region) = (&shard_region.shard, &shard_region.region);
        let res = send_with_retry(with_remote_auth(self.client.get(format!("https://glz-{region}-1.{shard}.a.pvp.net/core-game/v1/players/{puuid}")), auth)).await?.error_for_status()?;
        log::debug!("current ingame response: {res:#?}");
        let res = res.text().await;
        log::debug!("current ingame response body: {res:#?}");
        serde_json::from_str(&res?).map_err(Into::into)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CurrentPlayerIngame {
    #[serde(rename = "MatchID")]
    pub match_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CurrentPlayerPregame {
    #[serde(rename = "MatchID")]
    pub match_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PregameMatch {
    #[serde(rename = "ID")]
    pub match_id: String,
    #[serde(rename = "MapID")]
    pub map_url: String,
}

fn with_remote_auth(
    req: RequestBuilder,
    auth: &ValorantClientAuth,
) -> RequestBuilder {
    req.bearer_auth(&auth.access_token)
        .header(RIOT_ENTITLEMENTS_HEADER, &auth.token)
}

fn with_local_auth(req: RequestBuilder, lockfile: &Lockfile) -> RequestBuilder {
    req.basic_auth("riot", Some(lockfile.password.clone()))
}

async fn send_with_retry(req: RequestBuilder) -> Result<Response> {
    match req.try_clone().unwrap().send().await {
        Ok(ok) => Ok(ok),
        Err(err) => {
            if err.is_timeout() {
                req.send().await.map_err(Into::into)
            } else {
                Err(err.into())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PlayerInfo {
    #[serde(rename = "sub")]
    pub subject: String,
}
