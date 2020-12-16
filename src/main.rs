use std::{convert::TryFrom, net::IpAddr, sync::Arc};

use async_compat::Compat;
use futures::{pin_mut, StreamExt};
use http::Uri;
use lights_broadlink::discover;
use serde::{Deserialize, Serialize};
use smol::{block_on, lock::Mutex};
use warp::Filter;

#[derive(Deserialize, Debug)]
struct OauthQuery {
    client_id: String,
    redirect_uri: String,
    state: String,
    response_type: String,
    user_locale: String,
    scope: Option<String>,
}

#[derive(Deserialize, Debug)]
struct TokenQuery {
    client_id: String,
    client_secret: String,
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    token_type: String,
    access_token: String,
    refresh_token: String,
    expires_in: u32,
}

#[derive(Deserialize, Debug)]
struct Input {
    intent: String,
    payload: Option<IntentPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IntentPayload {
    Execute { commands: Vec<Command> },
}

#[derive(Debug, Deserialize)]
struct CommandDevice {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Command {
    devices: Vec<CommandDevice>,
    execution: Vec<CommandCommand>,
}

#[derive(Debug, Deserialize)]
struct CommandCommand {
    command: String,
    params: CommandParams,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum CommandParams {
    OnOff { on: bool },
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct FulfillmentRequest {
    request_id: String,
    inputs: Vec<Input>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FulfillmentResponse {
    request_id: String,
    payload: Payload,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Payload {
    agent_user_id: String,
    devices: Vec<Device>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Device {
    id: String,
    #[serde(rename = "type")]
    ty: String,
    traits: Vec<String>,
    name: Name,
    will_report_state: bool,
}

#[derive(Serialize)]
struct Name {
    name: String,
}

fn main() {
    let authorization_code = uuid::Uuid::new_v4().to_string();
    let access_token = authorization_code.clone();
    let refresh_token = access_token.clone();
    block_on(async move {
        let stream = discover();
        pin_mut!(stream);
        let target_addr: IpAddr = "192.168.4.186".parse().unwrap();
        let mut light = None;
        while let Some(Ok(item)) = stream.next().await {
            if item.addr() == target_addr {
                light = Some(item);
                break;
            }
        }
        let light = light.unwrap();
        let connection = Arc::new(Mutex::new(light.connect().await.unwrap()));

        let auth = warp::path("auth");
        let auth_init =
            auth.and(warp::path("auth"))
                .and(warp::query())
                .map(move |query: OauthQuery| {
                    warp::redirect::redirect(
                        Uri::try_from(format!(
                            "{}?code={}&state={}",
                            query.redirect_uri, authorization_code, query.state
                        ))
                        .unwrap(),
                    )
                });
        let auth_token =
            auth.and(warp::path("token"))
                .and(warp::body::form())
                .map(move |query: TokenQuery| {
                    println!("token req: {:?}", query);
                    warp::reply::json(&TokenResponse {
                        token_type: "Bearer".to_owned(),
                        access_token: access_token.clone(),
                        refresh_token: refresh_token.clone(),
                        expires_in: 10,
                    })
                });
        let auth = auth_init.or(auth_token);
        let fulfill =
            warp::path("fulfill")
                .and(warp::body::json())
                .map(move |data: FulfillmentRequest| {
                    println!("{:?}", data);
                    for input in &data.inputs {
                        if input.intent == "action.devices.EXECUTE" {
                            #[allow(irrefutable_let_patterns)]
                            if let Some(payload) = &input.payload {
                                if let IntentPayload::Execute { commands } = payload {
                                    for command in commands {
                                        for command in &command.execution {
                                            if let CommandParams::OnOff { on } = command.params {
                                                println!("on: {}", on);
                                                if on {
                                                    smol::spawn({
                                                        let connection = connection.clone();
                                                        async move {
                                                            connection
                                                                .lock()
                                                                .await
                                                                .turn_on()
                                                                .await
                                                                .unwrap();
                                                        }
                                                    })
                                                    .detach();
                                                } else {
                                                    smol::spawn({
                                                        let connection = connection.clone();
                                                        async move {
                                                            connection
                                                                .lock()
                                                                .await
                                                                .turn_off()
                                                                .await
                                                                .unwrap();
                                                        }
                                                    })
                                                    .detach();
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    warp::reply::json(&FulfillmentResponse {
                        request_id: data.request_id,
                        payload: Payload {
                            agent_user_id: "haha.yes".to_owned(),
                            devices: vec![Device {
                                id: "1".to_string(),
                                ty: "action.devices.types.LIGHT".into(),
                                traits: vec![
                                    "action.devices.traits.OnOff".into(),
                                    "action.devices.traits.ColorSetting".into(),
                                    "action.devices.traits.Brightness".into(),
                                ],
                                name: Name {
                                    name: "Gary".to_string(),
                                },
                                will_report_state: false,
                            }],
                        },
                    })
                });
        Compat::new(
            warp::serve(auth.or(fulfill).or(warp::any().map(|| {
                println!("fallback");
                format!("lol")
            })))
            .run(([127, 0, 0, 1], 8080)),
        )
        .await;
    });
}
