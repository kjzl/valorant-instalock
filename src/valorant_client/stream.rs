//! Wrapper over a Websocket connection to the local Valorant Client.
use futures::{SinkExt, Stream, StreamExt};
use strum::VariantArray;
use tokio::sync::mpsc::{
    error::{SendError, TrySendError},
    Receiver, Sender,
};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message};

use crate::lockfile::Lockfile;

use super::types::{
    ClientStatus, Event, EventKind, MessagingServiceMessage, ValorantClientAuth,
};
use serde::Deserialize;

type TokioWebsocketStream = tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
>;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum RelevantEvent {
    /*
    Object {
               "ackRequired": Bool(false),
               "id": String(""),
               "payload": String("{\"subject\":\"3e62cdbc-c4d0-5408-9de0-74bd1555f4cb\",\"cxnState\":\"CONNECTED\",\"clientID\":\"77789327-84cc-493d-92ec-d58d21dec5d9\",\"clientVersion\":\"release-08.05-shipping-13-2404755\",\"loopState\":\"MENUS\",\"loopStateMetadata\":\"\",\"version\":9,\"lastHeartbeatTime\":\"2024-03-31T14:31:57.282Z\",\"expiredTime\":\"0001-01-01T00:00:00Z\",\"heartbeatIntervalMillis\":60000,\"playtimeNotification\":\"\",\"playtimeMinutes\":4,\"isRestricted\":false,\"userinfoValidTime\":\"0001-01-01T00:00:00Z\",\"restrictionType\":\"\",\"clientPlatformInfo\":{\"platformType\":\"PC\",\"platformOS\":\"Windows\",\"platformOSVersion\":\"10.0.22621.1.256.64bit\",\"platformChipset\":\"Unknown\"},\"connectionTime\":\"2024-03-31T14:27:15.146Z\",\"shouldForceInvalidate\":false}"),
               "resource": String("ares-session/v1/sessions/3e62cdbc-c4d0-5408-9de0-74bd1555f4cb"),
               "service": String("session"),
               "timestamp": Number(1711895523998),
               "version": String("9"),
           }
    */
    ClientInfo(Event<MessagingServiceMessage<ClientStatus>>),
    EntitlementsToken(Event<ValorantClientAuth>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValorantEvent {
    /*
        [
       8,
       "OnJsonApiEvent_entitlements_v1_token",
       {
          "data":{
             "accessToken":"eyJraWQiOiJzMSIsImFsZyI6IlJTMjU2In0.eyJwcCI6eyJjIjoiZXUifSwic3ViIjoiM2U2MmNkYmMtYzRkMC01NDA4LTlkZTAtNzRiZDE1NTVmNGNiIiwic2NwIjpbIm9wZW5pZCIsImxpbmsiLCJiYW4iLCJsb2xfcmVnaW9uIiwibG9sIiwic3VtbW9uZXIiLCJvZmZsaW5lX2FjY2VzcyJdLCJjbG0iOlsibG9sX2FjY291bnQiLCJlbWFpbF92ZXJpZmllZCIsIm9wZW5pZCIsInB3IiwibG9sIiwib3JpZ2luYWxfcGxhdGZvcm1faWQiLCJyZ25fRVVXMSIsInBob25lX251bWJlcl92ZXJpZmllZCIsInBob3RvIiwib3JpZ2luYWxfYWNjb3VudF9pZCIsInByZWZlcnJlZF91c2VybmFtZSIsImxvY2FsZSIsImJhbiIsImxvbF9yZWdpb24iLCJhY2N0X2dudCIsInJlZ2lvbiIsInB2cG5ldF9hY2NvdW50X2lkIiwiYWNjdCIsInVzZXJuYW1lIl0sImRhdCI6eyJwIjpudWxsLCJyIjoiRVVXMSIsImMiOiJlYzEiLCJ1IjoyNjc3OTE1Njc0MjgxNTY4LCJsaWQiOiJTWDVUZThvbDFKY2w1ajJ0b21iclRnIn0sImlzcyI6Imh0dHBzOi8vYXV0aC5yaW90Z2FtZXMuY29tIiwicGx0Ijp7ImRldiI6InVua25vd24iLCJpZCI6IndpbmRvd3MifSwiZXhwIjoxNzExOTA1OTAzLCJpYXQiOjE3MTE5MDIzMDMsImp0aSI6IkZhd2RTZWQyWVg0IiwiY2lkIjoicmlvdC1jbGllbnQifQ.a3zTW4erXGwE4nhNgOc4z7uc_o52n3E961ugajnteKND1kKwQk6hl7Tm7NqJDmXie2lD3aBYHXzdyUfFqGLFJyMyy4I94RDDvGZgD5_-nKIYTRFTbrHcW-80UoW5WJJZFiSfGUSuWzafHD2kC4dHGfaW0K-oK9oTw_gZZrBxO7k",
             "entitlements":[

             ],
             "issuer":"https://entitlements.auth.riotgames.com",
             "subject":"3e62cdbc-c4d0-5408-9de0-74bd1555f4cb",
             "token":"eyJraWQiOiJrMSIsImFsZyI6IlJTMjU2In0.eyJlbnRpdGxlbWVudHMiOltdLCJhdF9oYXNoIjoiTklldWpoTW83SVVmMzdHUjBUM0J5ZyIsInN1YiI6IjNlNjJjZGJjLWM0ZDAtNTQwOC05ZGUwLTc0YmQxNTU1ZjRjYiIsImlzcyI6Imh0dHBzOlwvXC9lbnRpdGxlbWVudHMuYXV0aC5yaW90Z2FtZXMuY29tIiwiaWF0IjoxNzExOTAyMzAzLCJqdGkiOiJGYXdkU2VkMllYNCJ9.egtRB8B6MZvIbe0jAdlebouL2bgFLDbQBkjE4SNvvDmFjkbR9cTyFrCPTiAfu-JOov66pljPiD94-0AZP6CGYHEDacGGJr7hZsgK9XkRFmlrd553T3YRTjsUkI0ZsV3cl5_orH3e9njUtmRyLnLM21pFwpLSA9fuWs5ZV27CcUqL4NYh3D_bNDaL8JwHabp9tRzTmATRcWfSaGeH9aIAtskciW5p2SG6zARasZJJ8qmssvZ1ms6-iiR_P6ffXHHCRvmdguY9QZoskD3-jo-LuVevRiul7iyuuufLHjrNcUb5At1fM5CmsZk8yTWUpQgkeamojHS-Uy14kyd_AlTbWA"
          },
          "eventType":"Update",
          "uri":"/entitlements/v1/token"
       }
    ]
         */
    EntitlementsTokenChanged(ValorantClientAuth),
    ClientInfo(ClientStatus),
}

/// mem::drop is enough to close the underlying stream
pub struct ValorantEventStream {
    rx: Option<Receiver<ValorantEvent>>,
}

impl ValorantEventStream {
    pub async fn connect(lockfile: &Lockfile) -> anyhow::Result<Self> {
        log::info!("Connecting ValorantEventStream");
        let mut ws = connect_local_websocket(lockfile).await?;
        log::debug!("Subscribing to Valorant events {:?}", EventKind::VARIANTS);
        subscribe_val_events(&mut ws).await?;
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        proxy_ws_events(tx, ws);
        Ok(Self { rx: Some(rx) })
    }

	pub async fn next(&mut self) -> Option<ValorantEvent> {
		let Some(rx) = self.rx.as_mut() else {
			return None;
		};
		rx.recv().await
	}

    pub fn close(&mut self) {
        log::info!("Closing ValorantEventStream");
		let Some(mut rx) = self.rx.take() else {
			return;
		};
		rx.close();
    }
}

fn proxy_ws_events(tx: Sender<ValorantEvent>, mut ws: TokioWebsocketStream) {
    tokio::task::spawn(async move {
        let mut last_event: Option<RelevantEvent> = None;
        'receive: loop {
			log::trace!("Waiting for Websocket Message");
            let Some(event) = ws.next().await else {
                log::debug!("Websocket stream closed");
                break;
            };
            let event = match event {
                Ok(event) => event,
                Err(err) => {
                    log::error!(
                        "Websocket stream error (closing the stream): {:?}",
                        err
                    );
                    break;
                }
            };
            match event {
                msg @ Message::Binary(_) | msg @ Message::Text(_) => {
                    let text = msg.into_text().unwrap();
                    log::trace!("Received Websocket Message: {text}");
                    if text.is_empty() {
                        continue;
                    }
                    let val_event = match serde_json::from_str::<RelevantEvent>(
                        &text,
                    ) {
                        Ok(event) => {
                            // if event is the same as the last one, ignore it
                            // TODO remove this if this issue does not persist anymore
                            if let Some(last_event) = last_event.as_ref() {
                                if last_event == &event {
                                    log::info!(
                                        "Received duplicate event: {event:?}"
                                    );
                                    continue;
                                }
                            }
                            last_event = Some(event.clone());

                            match event {
                                RelevantEvent::EntitlementsToken(event) => {
                                    log::debug!("Received EntitlementsToken event: {event:#?}");
                                    ValorantEvent::EntitlementsTokenChanged(
                                        event.2.data,
                                    )
                                }
                                RelevantEvent::ClientInfo(event) => {
                                    log::debug!(
                                        "Received ClientInfo event: {event:#?}"
                                    );
                                    ValorantEvent::ClientInfo(
                                        event.2.data.payload,
                                    )
                                }
                            }
                        }
                        Err(err) => {
                            // should only happen for events we don't care about
                            log::trace!("Error while parsing event: {err}");
                            log::trace!("Event data: {text}");
                            continue;
                        }
                    };
					'send: loop {
						match tx.try_send(val_event.clone()) {
							Ok(_) => break 'send,
							Err(err @ TrySendError::Closed(_)) => {
								log::info!(
									"ValorantEventStream receiver closed: {err}"
								);
								let _ = ws.close(None).await;
								break 'receive;
							}
							Err(err @ TrySendError::Full(_)) => {
								log::warn!(
									"ValorantEventStream receiver full (waiting 100ms); Err msg: {err}"
								);
								tokio::time::sleep(std::time::Duration::from_millis(100)).await;
							}
						}
					};
                }
                Message::Close(info) => {
                    log::warn!(r#"Received "Websocket Close" Message"#);
                    if let Some(info) = info {
                        log::warn!("Details: {info}");
                    }
                    break;
                }
                _ => (),
            }
        }
    });
}

async fn connect_local_websocket(
    lockfile: &Lockfile,
) -> anyhow::Result<TokioWebsocketStream> {
    let mut request = lockfile.websocket_addr().into_client_request().unwrap();
    request
        .headers_mut()
        .insert(http::header::AUTHORIZATION, lockfile.auth_header());

    let (socket, _) = tokio_tungstenite::connect_async_tls_with_config(
        request,
        Some(
            tokio_tungstenite::tungstenite::protocol::WebSocketConfig::default(
            ),
        ),
        false,
        Some(tokio_tungstenite::Connector::NativeTls(
            native_tls::TlsConnector::builder()
                .danger_accept_invalid_certs(true)
                .danger_accept_invalid_hostnames(true)
                .build()
                .unwrap(),
        )),
    )
    .await?;
    Ok(socket)
}

async fn subscribe_val_events(
    ws: &mut TokioWebsocketStream,
) -> anyhow::Result<()> {
    let messages: Vec<tokio_tungstenite::tungstenite::Result<Message>> =
        EventKind::VARIANTS
            .iter()
            .map(|msg| {
                Ok(Message::Text(format!(
                    "[5, \"{}\"]",
                    <&EventKind as Into<&'static str>>::into(msg)
                )))
            }) // 5 is the code for subscribing to a certain event
            .collect();

    Ok(ws.send_all(&mut futures::stream::iter(messages)).await?)
}

#[cfg(test)]
mod test {
    use crate::valorant_client::types::{
        Command, DataModifier, Event, EventData, EventKind, GameLoopState,
        MessagingServiceMessage,
    };

    use super::RelevantEvent;

    const ENTITLEMENTS_TOKEN_MESSAGE: &str = r#"[8,"OnJsonApiEvent_entitlements_v1_token",{"data":"unknown contents","eventType":"Create","uri":"/riot-messaging-service/v1/messages/ares-pregame/pregame/v1/players/3e62cdbc-c4d0-5408-9de0-74bd1555f4cb"}]"#;
    const CLIENT_STATUS_MESSAGE: &str = r#"[8,"OnJsonApiEvent_riot-messaging-service_v1_message",{"data":{"ackRequired":false,"id":"","payload":"{\"subject\":\"3e62cdbc-c4d0-5408-9de0-74bd1555f4cb\",\"cxnState\":\"CONNECTED\",\"clientID\":\"60566def-5797-4b11-a138-6ec66bd6a6b5\",\"clientVersion\":\"release-06.08-shipping-19-875485\",\"loopState\":\"PREGAME\",\"loopStateMetadata\":\"affd0370-cd8b-4e7d-8998-ff88fb49b0ab\",\"version\":4,\"lastHeartbeatTime\":\"2023-05-16T17:52:41.061Z\",\"expiredTime\":\"0001-01-01T00:00:00Z\",\"heartbeatIntervalMillis\":60000,\"playtimeNotification\":\"\",\"playtimeMinutes\":139,\"isRestricted\":false,\"userinfoValidTime\":\"0001-01-01T00:00:00Z\",\"restrictionType\":\"\",\"clientPlatformInfo\":{\"platformType\":\"PC\",\"platformOS\":\"Windows\",\"platformOSVersion\":\"10.0.22621.1.256.64bit\",\"platformChipset\":\"Unknown\"}}","resource":"ares-session/v1/sessions/3e62cdbc-c4d0-5408-9de0-74bd1555f4cb","service":"session","timestamp":1684259598380,"version":"4"},"eventType":"Create","uri":"/riot-messaging-service/v1/message/ares-session/v1/sessions/3e62cdbc-c4d0-5408-9de0-74bd1555f4cb"}]"#;

    const SUBSCRIBE_COMMAND: &str =
        r#"[5,"OnJsonApiEvent_entitlements_v1_token"]"#;

    #[derive(Debug, serde::Deserialize)]
    #[serde(untagged)]
    enum TestWrapperEventEnum {
        MyEvent(Event<String>),
    }

    #[test]
    fn test_event_wrapper_enum() {
        let event: TestWrapperEventEnum = serde_json::from_str(
            r#"[8,"OnJsonApiEvent_entitlements_v1_token",{"data":"my data","eventType":"Create","uri":"my uri"}]"#,
        )
        .unwrap();
        match event {
            TestWrapperEventEnum::MyEvent(event) => {
                assert_eq!(event.0, 8);
                assert_eq!(event.1, EventKind::EntitlementsToken);
                assert_eq!(event.2.data, "my data");
            }
        }
    }

    #[test]
    fn test_parse_event_generic() {
        let data: EventData<String> = serde_json::from_str(
            r#"{"data":"my data","eventType":"Create","uri":"my uri"}"#,
        )
        .unwrap();
        assert_eq!(data.data, "my data");
        assert_eq!(data.event_type, DataModifier::Create);
        assert_eq!(data.uri, "my uri");
        let event: Event<String> = serde_json::from_str(
            r#"[8,"OnJsonApiEvent_riot-messaging-service_v1_message",{"data":"my data","eventType":"Create","uri":"my uri"}]"#,
        ).unwrap();
        assert_eq!(event.0, 8);
        assert_eq!(event.1, EventKind::MessagingService);
        assert_eq!(event.2.data, "my data");
    }

    #[test]
    fn test_parse_entitlements_event() {
        let entitlements_event: RelevantEvent =
            serde_json::from_str(ENTITLEMENTS_TOKEN_MESSAGE).unwrap();
        match entitlements_event {
            RelevantEvent::EntitlementsToken(event) => {
                assert_eq!(event.0, 8);
                assert_eq!(event.1, EventKind::EntitlementsToken);
            }
            _ => panic!("Expected EntitlementsToken event"),
        }
    }

    #[test]
    fn test_parse_messaging_service_message() {
        let message_event: Event<MessagingServiceMessage<String>> = serde_json::from_str(
            r#"[8,"OnJsonApiEvent_riot-messaging-service_v1_message",{"data":{"payload":"\"empty payload\""},"eventType":"Create","uri":"my uri"}]"#,
        ).unwrap();
        assert_eq!(message_event.0, 8);
        assert_eq!(message_event.1, EventKind::MessagingService);
        assert_eq!(message_event.2.data.payload, "empty payload");
    }

    #[test]
    fn test_parse_client_status_event() {
        let client_status_event: RelevantEvent =
            serde_json::from_str(CLIENT_STATUS_MESSAGE).unwrap();
        match client_status_event {
            RelevantEvent::ClientInfo(event) => {
                assert_eq!(event.0, 8);
                assert_eq!(event.1, EventKind::MessagingService);
                assert_eq!(
                    event.2.data.payload.loop_state,
                    GameLoopState::Pregame
                );
                assert_eq!(
                    event.2.data.payload.maybe_match_id,
                    "affd0370-cd8b-4e7d-8998-ff88fb49b0ab"
                );
            }
            _ => panic!("Expected ClientInfo event"),
        }
    }

    #[test]
    fn test_subscribe_command() {
        let subscribe_command =
            Command::new_subscribe(EventKind::EntitlementsToken);
        assert_eq!(
            serde_json::to_string(&subscribe_command).unwrap(),
            SUBSCRIBE_COMMAND
        );
    }
}
