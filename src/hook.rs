use crate::{format_list, App};
use serde::{Deserialize, Serialize};
use std::iter;
use warp::Rejection;

impl warp::reject::Reject for SerdeRejection {}

#[derive(Debug)]
struct SerdeRejection(serde_json::Error);

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum HandlerNameRaw {
    ListGroups,
    EnumerateGroup,
    RenameLight,
    IsLightInGroup,
    NewGroup,
}

#[derive(Deserialize)]
struct HookHandler {
    name: HandlerNameRaw,
}

#[derive(Debug, Clone)]
enum HandlerCommand {
    ListGroups,
    EnumerateGroup { name: String },
    RenameLight,
    IsLightInGroup,
    NewGroup { name: String, lights: Vec<String> },
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
                HandlerNameRaw::NewGroup => HandlerCommand::NewGroup {
                    name: scene
                        .slot_as_str("name")
                        .ok_or(serde::de::Error::custom(format!("`name` slot invalid")))?,
                    lights: scene
                        .slot_as_array("lights")
                        .ok_or(serde::de::Error::custom(format!("`lights` slot invalid")))?,
                },
                HandlerNameRaw::ListGroups => HandlerCommand::ListGroups,
                HandlerNameRaw::RenameLight => HandlerCommand::RenameLight,
                HandlerNameRaw::IsLightInGroup => HandlerCommand::IsLightInGroup,
                HandlerNameRaw::EnumerateGroup => {
                    let name = intent
                        .param_as_str("group")
                        .ok_or(serde::de::Error::custom(format!("`group` param invalid")))?;
                    HandlerCommand::EnumerateGroup { name }
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
    fn build<I: IntoIterator<Item = T>, T: AsRef<str>>(self, lights: I) -> HookResponse {
        HookResponse {
            session: HookResponseSession {
                id: self.session.id,
                type_overrides: vec![TypeOverride {
                    name: "light".into(),
                    type_override_mode: TypeOverrideMode::TypeReplace,
                    synonym: TypeSynonym {
                        entries: lights
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

pub async fn hook(input: HookData, app: &mut App) -> Result<String, Rejection> {
    let command = input.command;
    let session = input.session;
    serde_json::to_string(
        &session
            .make_response(&match command {
                HandlerCommand::ListGroups => {
                    let mut groups = app.groups().peekable();
                    let len = groups.len();
                    if let Some(group) = groups.peek() {
                        if len == 1 {
                            format!("You have one light group called {}.", group)
                        } else {
                            format!(
                                "You have {} light groups{}{}.",
                                len,
                                if len > 3 {
                                    ". The first three are: "
                                } else {
                                    ": "
                                },
                                format_list(groups.take(3))
                            )
                        }
                    } else {
                        "You don't have any light groups.".to_owned()
                    }
                }
                HandlerCommand::NewGroup { name, lights } => {
                    let name = name.trim().to_lowercase();
                    let mut absent_lights = app.absent(lights.iter()).peekable();
                    if let Some(light) = absent_lights.next() {
                        if let None = absent_lights.peek() {
                            format!("Light {} does not exist.", light)
                        } else {
                            format!(
                                "Lights {} do not exist.",
                                format_list(iter::once(light).chain(absent_lights))
                            )
                        }
                    } else {
                        if app.has_group(&name) {
                            format!("Group {} already exists.", name)
                        } else {
                            drop(absent_lights);
                            app.add_lights(&name, lights);
                            format!("Alright, group created.")
                        }
                    }
                }
                HandlerCommand::EnumerateGroup { name } => {
                    if let Some(lights) = app.group(&name) {
                        let mut iter = lights.iter().peekable();
                        if iter.len() == 1 {
                            format!(
                                "Group {} contains only light {}.",
                                name,
                                iter.next().unwrap().name()
                            )
                        } else {
                            format!(
                                "Group {} contains lights {}.",
                                name,
                                format_list(iter.map(|iter| iter.name()))
                            )
                        }
                    } else {
                        format!("Group {} does not exist.", name)
                    }
                }
                _ => format!("idk that"),
            })
            .build(app.light_names()),
    )
    .map_err(|e| warp::reject::custom(SerdeRejection(e)))
}
