use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{App, Color};

#[derive(Deserialize, Debug)]
struct Input {
    intent: String,
    payload: Option<IntentPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum IntentPayload {
    Execute { commands: Vec<Command> },
    Query { devices: Vec<CommandDevice> },
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
    Brightness { brightness: u8 },
    Color { color: QueryColor },
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
    payload: Option<Payload>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
enum Payload {
    Sync {
        agent_user_id: String,
        devices: Vec<Device>,
    },
    Query {
        agent_user_id: String,
        devices: HashMap<String, QueryDevice>,
    },
    Execute {
        commands: Vec<ExecCommand>,
    },
}

#[derive(Serialize, Clone)]
struct ExecCommand {
    ids: Vec<String>,
    status: String,
    states: ExecStates,
}

#[derive(Serialize, Clone)]
struct ExecStates {
    online: bool,
}

#[derive(Serialize, Clone)]
struct QueryDevice {
    status: String,
    online: bool,
    brightness: u8,
    on: bool,
    color: QueryColor,
}

#[derive(Serialize, Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
enum QueryColor {
    White {
        temperature: u32,
        name: String,
    },
    Rgb {
        #[serde(rename = "spectrumRGB")]
        spectrum_rgb: u32,
        name: String,
    },
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Device {
    id: String,
    #[serde(rename = "type")]
    ty: String,
    traits: Vec<String>,
    name: Name,
    will_report_state: bool,
    attributes: DeviceAttributes,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DeviceAttributes {
    color_model: String,
    color_temperature_range: ColorTemperatureRange,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ColorTemperatureRange {
    temperature_min_k: u32,
    temperature_max_k: u32,
}

#[derive(Serialize, Clone)]
struct Name {
    name: String,
}

pub async fn fulfill(request: FulfillmentRequest, app: &mut App) -> FulfillmentResponse {
    let mut payload = None;
    for input in &request.inputs {
        if input.intent == "action.devices.SYNC" {
            payload = Some(Payload::Sync {
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
                        attributes: DeviceAttributes {
                            color_model: "rgb".to_owned(),
                            color_temperature_range: ColorTemperatureRange {
                                temperature_min_k: 2000,
                                temperature_max_k: 7500,
                            },
                        },
                    })
                    .collect(),
            });
            break;
        } else if input.intent == "action.devices.EXECUTE" {
            let mut exec_commands = vec![];
            if let Some(payload) = &input.payload {
                if let IntentPayload::Execute { commands } = payload {
                    for command in commands {
                        exec_commands.push(ExecCommand {
                            ids: command.devices.iter().map(|item| item.id.clone()).collect(),
                            status: "SUCCESS".to_owned(),
                            states: ExecStates { online: true },
                        });
                        for device in &command.devices {
                            for command in &command.execution {
                                match &command.params {
                                    CommandParams::OnOff { on } => {
                                        let _ = app.set_state(&device.id, (*on).into()).await;
                                    }
                                    CommandParams::Brightness { brightness } => {
                                        let _ = app
                                            .set_brightness(
                                                &device.id,
                                                ((*brightness as f32 / 100.) * 255.) as u8,
                                            )
                                            .await;
                                    }
                                    CommandParams::Color { color } => match color {
                                        QueryColor::Rgb { spectrum_rgb, .. } => {
                                            let color = format!("{:06X}", spectrum_rgb)
                                                .as_bytes()
                                                .chunks(2)
                                                .map(|byte| {
                                                    u8::from_str_radix(
                                                        &format!(
                                                            "{}{}",
                                                            byte[0] as char, byte[1] as char
                                                        ),
                                                        16,
                                                    )
                                                    .unwrap()
                                                })
                                                .collect::<Vec<_>>();
                                            let _ = app
                                                .set_color(
                                                    &device.id,
                                                    Color::Rgb {
                                                        r: color[0],
                                                        g: color[1],
                                                        b: color[2],
                                                    },
                                                )
                                                .await;
                                        }
                                        QueryColor::White { temperature, .. } => {
                                            let temperature = *temperature;
                                            let _ = app
                                                .set_color(&device.id, Color::White { temperature })
                                                .await;
                                        }
                                    },
                                }
                            }
                        }
                    }
                }
            }
            payload = Some(Payload::Execute {
                commands: exec_commands,
            });
            break;
        } else if input.intent == "action.devices.QUERY" {
            if let Some(loc_payload) = &input.payload {
                if let IntentPayload::Query { devices } = loc_payload {
                    payload = Some(Payload::Query {
                        agent_user_id: "haha.yes".to_owned(),
                        devices: app
                            .lights()
                            .filter_map(|device| {
                                let id = device.id();
                                if devices.iter().any(|dev| dev.id == id) {
                                    Some((
                                        id,
                                        QueryDevice {
                                            online: true,
                                            brightness: ((device.brightness() as f32 / 255.) * 100.)
                                                as u8,
                                            on: device.is_on(),
                                            status: "SUCCESS".to_owned(),
                                            color: QueryColor::Rgb {
                                                name: "".to_owned(),
                                                spectrum_rgb: device.color(),
                                            },
                                        },
                                    ))
                                } else {
                                    None
                                }
                            })
                            .collect(),
                    });
                }
            }
            break;
        }
    }
    FulfillmentResponse {
        request_id: request.request_id,
        payload,
    }
}
