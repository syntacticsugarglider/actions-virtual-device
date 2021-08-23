use serde::{Deserialize, Serialize};
use warp::Rejection;

impl warp::reject::Reject for SerdeRejection {}

#[derive(Debug)]
struct SerdeRejection(serde_json::Error);

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum HandlerNameRaw {
    RunProgram,
}

#[derive(Deserialize)]
struct HookHandler {
    name: HandlerNameRaw,
}

#[derive(Debug, Clone)]
enum HandlerCommand {
    RunProgram { program: String },
}

impl<'de> Deserialize<'de> for HookData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let HookRequest {
            session: HookSession { id },
            intent,
            handler: HookHandler { name },
            scene,
        } = HookRequest::deserialize(deserializer)?;
        Ok(HookData {
            session: id,
            command: match name {
                HandlerNameRaw::RunProgram => {
                    let program = intent
                        .param_as_str("program")
                        .ok_or(serde::de::Error::custom(format!("`program` param invalid")))?;
                    HandlerCommand::RunProgram { program }
                }
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct HookData {
    session: SessionId,
    command: HandlerCommand,
}

#[derive(Deserialize, Debug, Serialize, Clone)]
struct HookSession {
    id: SessionId,
}

#[derive(Deserialize, Debug)]
struct SlotValue {
    value: String,
}

#[derive(Deserialize, Debug)]
struct HookScene {
    slots: serde_json::Value,
}

#[derive(Deserialize, Debug)]
struct HookIntent {
    params: serde_json::Value,
}

#[derive(Deserialize)]
struct HookRequest {
    handler: HookHandler,
    session: HookSession,
    scene: HookScene,
    intent: HookIntent,
}

impl HookScene {
    fn slot_as_str(&self, slot: &str) -> Option<String> {
        self.slots
            .get(slot)
            .map(|item| item.get("value"))
            .flatten()
            .map(|item| item.as_str())
            .flatten()
            .map(str::to_owned)
    }
    fn slot_as_array(&self, slot: &str) -> Option<Vec<String>> {
        self.slots
            .get(slot)
            .map(|item| item.get("value"))
            .flatten()
            .map(|item| item.as_array())
            .flatten()
            .map(|item| {
                item.into_iter()
                    .map(|item| item.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()
            })
            .flatten()
    }
}

impl HookIntent {
    fn param_as_str(&self, param: &str) -> Option<String> {
        self.params
            .get(param)
            .map(|item| item.get("resolved"))
            .flatten()
            .map(|item| item.as_str())
            .flatten()
            .map(str::to_owned)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(transparent)]
struct SessionId(String);

impl SessionId {
    fn make_response(self, data: &str) -> HookResponseBuilder {
        HookResponseBuilder {
            session: HookSession { id: self },
            prompt: HookPrompt {
                first_simple: SimplePrompt {
                    speech: data.into(),
                    text: None,
                },
            },
        }
    }
}

#[derive(Serialize)]
struct SimplePrompt {
    speech: String,
    text: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HookPrompt {
    first_simple: SimplePrompt,
}

#[derive(Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum TypeOverrideMode {
    TypeReplace,
}

struct HookResponseBuilder {
    session: HookSession,
    prompt: HookPrompt,
}

#[derive(Serialize)]
struct TypeEntry {
    name: String,
    synonyms: Vec<String>,
}

#[derive(Serialize)]
struct TypeSynonym {
    entries: Vec<TypeEntry>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TypeOverride {
    name: String,
    synonym: TypeSynonym,
    type_override_mode: TypeOverrideMode,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HookResponse {
    session: HookResponseSession,
    prompt: HookPrompt,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HookResponseSession {
    type_overrides: Vec<TypeOverride>,
    id: SessionId,
}

impl HookResponseBuilder {
    fn build<I: IntoIterator<Item = T>, T: AsRef<str>>(self, programs: I) -> HookResponse {
        HookResponse {
            session: HookResponseSession {
                id: self.session.id,
                type_overrides: vec![TypeOverride {
                    name: "program".into(),
                    type_override_mode: TypeOverrideMode::TypeReplace,
                    synonym: TypeSynonym {
                        entries: programs
                            .into_iter()
                            .map(|light| {
                                let name = light.as_ref().to_owned();
                                TypeEntry {
                                    name: name.to_owned(),
                                    synonyms: vec![name.to_lowercase().into()],
                                }
                            })
                            .collect(),
                    },
                }],
            },
            prompt: self.prompt,
        }
    }
}

pub async fn hook(input: HookData) -> Result<String, Rejection> {
    let command = input.command;
    let session = input.session;
    serde_json::to_string(
        &session
            .make_response(&match command {
                HandlerCommand::RunProgram { program } => {
                    if let Ok(_) = surf::post(format!(
                        "http://lightsmanager.syntacticsugarglider.com/upload/{}/192.168.4.203",
                        std::env::var("ESP_AUTH_TOKEN").unwrap_or("".into())
                    ))
                    .body(surf::Body::from_bytes(
                        std::fs::read(format!("programs/{}.wasm", program)).unwrap_or(vec![]),
                    ))
                    .send()
                    .await
                    {
                        format!("Done.")
                    } else {
                        format!("Something went wrong lol")
                    }
                }
                _ => format!("Something went wrong lmao"),
            })
            .build(
                std::fs::read_dir("./programs")
                    .unwrap()
                    .into_iter()
                    .map(|path| {
                        format!(
                            "{}",
                            path.unwrap()
                                .path()
                                .file_name()
                                .unwrap()
                                .to_str()
                                .unwrap()
                                .split('.')
                                .next()
                                .unwrap()
                        )
                    }),
            ),
    )
    .map_err(|e| warp::reject::custom(SerdeRejection(e)))
}
