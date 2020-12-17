use serde::{Deserialize, Serialize};

use crate::App;

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
pub struct FulfillmentRequest {
    request_id: String,
    inputs: Vec<Input>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FulfillmentResponse {
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

pub async fn fulfill(request: FulfillmentRequest, app: &mut App) -> FulfillmentResponse {
    for input in &request.inputs {
        if input.intent == "action.devices.EXECUTE" {
            #[allow(irrefutable_let_patterns)]
            if let Some(payload) = &input.payload {
                if let IntentPayload::Execute { commands } = payload {
                    for command in commands {
                        for device in &command.devices {
                            for command in &command.execution {
                                if let CommandParams::OnOff { on } = command.params {
                                    let result = app.set_state(&device.id, on.into()).await;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    FulfillmentResponse {
        request_id: request.request_id,
        payload: Payload {
            agent_user_id: "haha.yes".to_owned(),
            devices: app
                .lights()
                .map(|light| Device {
                    id: light.id(),
                    ty: "action.devices.types.LIGHT".into(),
                    traits: vec![
                        "action.devices.traits.OnOff".into(),
                        "action.devices.traits.ColorSetting".into(),
                        "action.devices.traits.Brightness".into(),
                    ],
                    name: Name { name: light.name() },
                    will_report_state: false,
                })
                .collect(),
        },
    }
}
