// "OnJsonApiEvent",
// "OnJsonApiEvent_riot-messaging-service_v1_messages",
// "OnJsonApiEvent_riot-messaging-service_v1_out-of-sync",
// "OnJsonApiEvent_riot-messaging-service_v1_session",
// "OnJsonApiEvent_riot-messaging-service_v1_state",
// "OnJsonApiEvent_riot-messaging-service_v1_user",

use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    strum::VariantArray,
    strum::IntoStaticStr,
    strum::EnumString,
    Deserialize,
    Serialize,
)]
#[serde(try_from = "&str", into = "&'static str")]
pub enum EventKind {
    #[strum(serialize = "OnJsonApiEvent_entitlements_v1_token")]
    EntitlementsToken,
    #[strum(serialize = "OnJsonApiEvent_riot-messaging-service_v1_message")]
    MessagingService,
    //#[strum(serialize = "OnJsonApiEvent")]
    //All,
}

#[derive(
    Debug, Copy, Clone, PartialEq, Eq, Serialize_repr, Deserialize_repr,
)]
#[repr(u8)]
pub enum OpCode {
    // Client -> Server
    Subscribe = 5,
    // Client -> Server
    Unsubscribe = 6,
    // Server -> Client
    Event = 8,
}

/// Client -> Server
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(bound(serialize = "T: Serialize"))]
pub struct Command<T>(pub OpCode, pub T);

/// Server -> Client
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct Event<T>(
    pub i32, /* message opcode = 8 for message sent by server to client */
    pub EventKind,
    pub EventData<T>,
);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", bound(deserialize = "T: Deserialize<'de>"))]
pub struct EventData<T> {
    pub data: T,
    pub event_type: DataModifier,
    pub uri: String,
}

#[derive(Debug, Copy, PartialEq, Eq, Clone, Deserialize)]
pub enum DataModifier {
    Update,
    Create,
    Delete,
}

impl Command<EventKind> {
    pub fn new_subscribe(event_kind: EventKind) -> Self {
        Self(OpCode::Subscribe, event_kind)
    }

    pub fn new_unsubscribe(event_kind: EventKind) -> Self {
        Self(OpCode::Unsubscribe, event_kind)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValorantClientAuth {
    pub access_token: String,
    // entitlement token
    pub token: String,
    // entitlements: Vec<String>,
    // issuer: Url,
    // subject: Uuid,
}

/*
{
    "ackRequired": false,
    "id": "",
    "payload": "{\"subject\":\"3e62cdbc-c4d0-5408-9de0-74bd1555f4cb\",\"cxnState\":\"CONNECTED\",\"clientID\":\"60566def-5797-4b11-a138-6ec66bd6a6b5\",\"clientVersion\":\"release-06.08-shipping-19-875485\",\"loopState\":\"PREGAME\",\"loopStateMetadata\":\"affd0370-cd8b-4e7d-8998-ff88fb49b0ab\",\"version\":4,\"lastHeartbeatTime\":\"2023-05-16T17:52:41.061Z\",\"expiredTime\":\"0001-01-01T00:00:00Z\",\"heartbeatIntervalMillis\":60000,\"playtimeNotification\":\"\",\"playtimeMinutes\":139,\"isRestricted\":false,\"userinfoValidTime\":\"0001-01-01T00:00:00Z\",\"restrictionType\":\"\",\"clientPlatformInfo\":{\"platformType\":\"PC\",\"platformOS\":\"Windows\",\"platformOSVersion\":\"10.0.22621.1.256.64bit\",\"platformChipset\":\"Unknown\"}}",
    "resource": "ares-session/v1/sessions/3e62cdbc-c4d0-5408-9de0-74bd1555f4cb",
    "service": "session",
    "timestamp": 1684259598380,
    "version": "4"
}
 */
/*
{
    "ackRequired": false,
    "id": "",
    "payload": "",
    "resource": "ares-pregame/pregame/v1/players/3e62cdbc-c4d0-5408-9de0-74bd1555f4cb",
    "service": "pregame",
    "timestamp": 1684259598375,
    "version": "1684259598371"
}
 */
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    rename_all = "camelCase",
    bound(deserialize = "T: serde::de::DeserializeOwned")
)]
pub struct MessagingServiceMessage<T> {
    /// The payload is a JSON string
    #[serde(deserialize_with = "deserialize_from_stringified_json")]
    pub payload: T,
    // ack_required: bool,
    // id: String,
    // resource: String,
    // service: String,
    // timestamp: u64,
    // version: String,
}

fn deserialize_from_stringified_json<'de, D, T>(
    deserializer: D,
) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let str: String = Deserialize::deserialize(deserializer)?;
    serde_json::from_str(&str).map_err(serde::de::Error::custom)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub subject: String,
    pub loop_state: GameLoopState,
    /// match_id or empty string
    #[serde(rename = "loopStateMetadata")]
    pub maybe_match_id: String,
    // subject: String,
    // cxn_state: String,
    // client_id: String,
    // client_version: String,
    // version: u32,
    // last_heartbeat_time: String,
    // expired_time: String,
    // heartbeat_interval_millis: u32,
    // playtime_notification: String,
    // playtime_minutes: u32,
    // is_restricted: bool,
    // userinfo_valid_time: String,
    // restriction_type: String,
    // client_platform_info: ClientPlatformInfo,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum GameLoopState {
    Pregame,
    Ingame,
    Menus,
}
