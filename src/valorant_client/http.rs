use std::collections::HashMap;

use crate::lockfile::Lockfile;
use anyhow::{Context, Result};
use reqwest::{Client, RequestBuilder, Response};
use serde::Deserialize;

use super::{types::ValorantClientAuth, ValorantClient};

const RIOT_ENTITLEMENTS_HEADER: &str = "X-Riot-Entitlements-JWT";
const RIOT_CLIENT_VERSION_HEADER: &str = "X-Riot-ClientVersion";
const RIOT_CLIENT_PLATFORM_HEADER: &str = "X-Riot-ClientPlatform";

/// https://127.0.0.1:{port}/product-session/v1/external-sessions

impl ValorantClient {
    pub async fn sessions_info(
        client: &Client,
        lockfile: &Lockfile,
    ) -> Result<SessionsResponse> {
        log::debug!("Sending session info request. lockfile: {:#?}", lockfile);
        let res = send_with_retry(with_local_auth(
            client.get(format!(
                "{}product-session/v1/external-sessions",
                lockfile.http_addr()
            )),
            lockfile,
        ))
        .await?
        .error_for_status()?
        .text()
        //.json()
        .await?;
        log::debug!("sessions info response: {:#?}", res);
        serde_json::from_str(&res).map_err(Into::into)
    }

    pub async fn fetch_auth_tokens(
        client: &Client,
        lockfile: &Lockfile,
    ) -> Result<ValorantClientAuth> {
        log::debug!("Sending auth tokens request. lockfile: {:#?}", lockfile);
        let res = send_with_retry(with_local_auth(
            client
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

    pub async fn quit_pregame(&self) -> Result<()> {
        log::debug!(
            "Sending quit pregame match request: {}",
            self.current_match_id()
                .as_ref()
                .context("No MatchID available")?
        );
        let res =
            send_with_retry(self.with_remote_auth(self.client.post(format!(
                "https://glz-{}-1.{}.a.pvp.net/pregame/v1/matches/{}/quit",
                &self.region,
                &self.shard,
                self.current_match_id()
                    .as_ref()
                    .context("No MatchID available")?
            ))))
            .await?
            .error_for_status()?;
        log::debug!("quit pregame response: {res:#?}");
        log::debug!("quit pregame response body: {:#?}", res.text().await);
        Ok(())
    }

    pub async fn lock_agent(&self, agent_id: &str) -> Result<()> {
        log::debug!(
            "Sending lock agent request: {agent_id}, {}",
            self.current_match_id()
                .as_ref()
                .context("No MatchID available")?
        );
        let res = send_with_retry(self.with_remote_auth(self.client.post(format!("https://glz-{}-1.{}.a.pvp.net/pregame/v1/matches/{}/lock/{agent_id}",
		&self.region,
		&self.shard,
		self.current_match_id()
			.as_ref()
			.context("No MatchID available")?)))).await?.error_for_status()?;
        log::debug!("lock agent response: {res:#?}");
        log::debug!("lock agent response body: {:#?}", res.text().await);
        Ok(())
    }

    // https://auth.riotgames.com/userinfo
    /*pub async fn current_player(
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
    }*/

    pub async fn get_pregame_match(&self) -> Result<PregameMatch> {
        log::debug!(
            "Sending get pregame match request: {}",
            &self
                .current_match_id()
                .as_ref()
                .context("No MatchID available")?
        );
        let res =
            send_with_retry(self.with_remote_auth(self.client.get(format!(
                "https://glz-{}-1.{}.a.pvp.net/pregame/v1/matches/{}",
                &self.region,
                &self.shard,
                self.current_match_id()
                    .as_ref()
                    .context("No MatchID available")?
            ))))
            .await?
            .error_for_status()?;
        log::debug!("get pregame match response: {res:#?}");
        let res = res.text().await;
        log::debug!("get pregame match response body: {res:#?}");
        serde_json::from_str(&res?).map_err(Into::into)
    }

    //https://glz-{region}-1.{shard}.a.pvp.net/pregame/v1/players/{puuid}
    pub async fn current_pregame(&self) -> Result<CurrentPlayerPregame> {
        log::debug!("Sending current pregame match request: {}", &self.subject);
        let res =
            send_with_retry(self.with_remote_auth(self.client.get(format!(
                "https://glz-{}-1.{}.a.pvp.net/pregame/v1/players/{}",
                &self.region, &self.shard, &self.subject
            ))))
            .await?
            .error_for_status()?;
        log::debug!("current pregame response: {res:#?}");
        let res = res.text().await;
        log::debug!("current pregame response body: {res:#?}");
        serde_json::from_str(&res?).map_err(Into::into)
    }

    //https://glz-{region}-1.{shard}.a.pvp.net/core-game/v1/players/{puuid}/disassociate/{current game match id}
    pub async fn quit_ingame(&self) -> Result<()> {
        log::debug!(
            "Sending quit ingame match request: {}",
            self.current_match_id()
                .as_ref()
                .context("No MatchID available")?
        );
        let res = send_with_retry(self.with_remote_auth(self.client.post(format!("https://glz-{}-1.{}.a.pvp.net/core-game/v1/players/{}/disassociate/{}", &self.region, &self.shard, &self.subject, self.current_match_id().as_ref().context("No MatchID available")?)))).await?.error_for_status()?;
        log::debug!("quit ingame response: {res:#?}");
        log::debug!("quit ingame response body: {:#?}", res.text().await);
        Ok(())
    }

    //https://glz-{region}-1.{shard}.a.pvp.net/core-game/v1/players/{puuid}
    pub async fn current_ingame(&self) -> Result<CurrentPlayerIngame> {
        log::debug!("Sending current ingame match request: {}", &self.subject);
        let res =
            send_with_retry(self.with_remote_auth(self.client.get(format!(
                "https://glz-{}-1.{}.a.pvp.net/core-game/v1/players/{}",
                &self.region, &self.shard, &self.subject
            ))))
            .await?
            .error_for_status()?;
        log::debug!("current ingame response: {res:#?}");
        let res = res.text().await;
        log::debug!("current ingame response body: {res:#?}");
        serde_json::from_str(&res?).map_err(Into::into)
    }

    fn with_remote_auth(&self, req: RequestBuilder) -> RequestBuilder {
        let auth = self.auth();
        req.bearer_auth(&auth.access_token)
            .header(RIOT_ENTITLEMENTS_HEADER, &auth.token)
            .header(RIOT_CLIENT_PLATFORM_HEADER, &self.platform)
            .header(RIOT_CLIENT_VERSION_HEADER, &self.version)
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

/*
type SessionsResponse = {
    [x: string]: {
        exitCode: number;
        exitReason: null;
        isInternal: boolean;
        launchConfiguration: {
            arguments: string[];
            executable: string;
            locale: string | null;
            voiceLocale: null;
            workingDirectory: string;
        };
        patchlineFullName: "VALORANT" | "riot_client";
        patchlineId: "" | "live" | "pbe";
        phase: string;
        productId: "valorant" | "riot_client";
        version: string;
    };
};
*/

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchConfiguration {
    // -config-endpoint=https://shared.eu.a.pvp.net
    // -ares-deployment=eu
    pub arguments: Vec<String>,
    //executable: String,
    //locale: Option<String>,
    //voiceLocale: Option<String>,
    //workingDirectory: String,
}

impl LaunchConfiguration {
    pub fn shard(&self) -> Option<String> {
        let re = regex::Regex::new(
            r"-config-endpoint=https://.*\.([a-z]+)\.a\.pvp\.net",
        )
        .unwrap();
        self.arguments.iter().find_map(|arg| {
            Some(re.captures(arg)?.get(1)?.as_str().to_string())
        })
    }

    pub fn region(&self) -> Option<String> {
        let re = regex::Regex::new(r"-ares-deployment=([a-z]+)").unwrap();
        self.arguments.iter().find_map(|arg| {
            Some(re.captures(arg)?.get(1)?.as_str().to_string())
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductId {
    Valorant,
    RiotClient,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub launch_configuration: LaunchConfiguration,
    pub version: String,
    pub product_id: ProductId,
    // , ...
}

pub type SessionsResponse = HashMap<String, SessionInfo>;

pub struct ClientInfo {
    /*
    Base64 encoded:
    {
    "platformType": "PC",
    "platformOS": "Windows",
    "platformOSVersion": "10.0.19042.1.256.64bit",
    "platformChipset": "Unknown"
    }
     */
    pub platform: String,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_shard() {
        let lc = LaunchConfiguration {
            arguments: vec![
                "-config-endpoint=https://shared.eu.a.pvp.net".to_string(),
                "-ares-deployment=eu".to_string(),
            ],
        };
        assert_eq!(lc.shard(), Some("eu".to_string()));
    }

    #[test]
    fn test_region() {
        let lc = LaunchConfiguration {
            arguments: vec![
                "-config-endpoint=https://shared.eu.a.pvp.net".to_string(),
                "-ares-deployment=eu".to_string(),
            ],
        };
        assert_eq!(lc.region(), Some("eu".to_string()));
    }
}
