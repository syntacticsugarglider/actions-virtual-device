use std::{
    borrow::Borrow, collections::HashMap, convert::TryFrom, fmt::Display, iter, net::IpAddr,
    sync::Arc,
};

use async_compat::Compat;
use futures::{pin_mut, StreamExt};
use http::Uri;
use lights_broadlink::discover;
use serde::{Deserialize, Serialize};
use smol::{block_on, lock::Mutex};
use warp::Filter;

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
#[derive(Deserialize, Debug)]
struct OauthQuery {
    client_id: String,
    redirect_uri: String,
    state: String,
    response_type: String,
    user_locale: String,
    scope: Option<String>,
}

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
struct HookData {
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

pub trait Light {
    fn name(&self) -> String;
}

pub struct App {
    lights: HashMap<String, Arc<Box<dyn Light + Sync + Send>>>,
    groups: HashMap<String, Vec<Arc<Box<dyn Light + Sync + Send>>>>,
}

impl App {
    fn groups(&self) -> impl ExactSizeIterator<Item = &String> {
        self.groups.keys()
    }
    fn lights(&self) -> impl ExactSizeIterator<Item = &String> {
        self.lights.keys()
    }
    fn add_lights(&mut self, group: &str, lights: impl IntoIterator<Item = String>) {
        let lights = lights
            .into_iter()
            .map(|light| self.lights.get(&light).unwrap().clone())
            .collect::<Vec<_>>();
        self.groups.get_mut(group).unwrap().extend(lights)
    }
    fn group(&self, group: &str) -> Option<&[Arc<Box<dyn Light + Sync + Send>>]> {
        self.groups.get(group).map(|a| a.as_slice())
    }
    fn has_group(&self, group: &str) -> bool {
        self.groups.contains_key(group)
    }
    fn absent<'a, T: Borrow<String>, I: Iterator<Item = T> + 'a>(
        &'a self,
        lights: I,
    ) -> impl Iterator<Item = T> + 'a {
        lights.filter(move |light| !self.lights.contains_key(light.borrow()))
    }
}

struct TestLight;

impl Light for TestLight {
    fn name(&self) -> String {
        format!("lol test light")
    }
}

impl TestLight {
    fn new() -> Arc<Box<dyn Light + Sync + Send>> {
        Arc::new(Box::new(TestLight))
    }
}

impl warp::reject::Reject for SerdeRejection {}

#[derive(Debug)]
struct SerdeRejection(serde_json::Error);

fn format_list<I: IntoIterator<Item = T>, T: Display>(data: I) -> String {
    let mut iter = data.into_iter().peekable();
    let mut data = String::new();
    if let Some(item) = iter.next() {
        data.push_str(&format!("{}", item))
    }
    if let Some(item) = iter.next() {
        if let None = iter.peek() {
            data.push_str(&format!(" and {}", item))
        } else {
            data.push_str(&format!(", {}", item))
        }
    }
    while let Some(item) = iter.next() {
        if let None = iter.peek() {
            data.push_str(&format!(", and {}", item))
        } else {
            data.push_str(&format!(", {}", item))
        }
    }
    data
}

fn main() {
    let authorization_code = uuid::Uuid::new_v4().to_string();
    let access_token = authorization_code.clone();
    let refresh_token = access_token.clone();
    block_on(async move {
        let app = Arc::new(Mutex::new(App {
            lights: vec![
                ("Gary".to_owned(), TestLight::new()),
                ("Doty".to_owned(), TestLight::new()),
            ]
            .into_iter()
            .collect(),
            groups: vec![
                ("Ebic Group".to_owned(), vec![]),
                ("Ebicer Group".to_owned(), vec![]),
                ("Ebicest Group".to_owned(), vec![]),
                ("Aaaaaaaa".to_owned(), vec![]),
            ]
            .into_iter()
            .collect(),
        }));

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
        let hook = warp::path("hook")
            .and(warp::body::bytes())
            .map(|data: bytes::Bytes| {
                let data = String::from_utf8_lossy(data.as_ref());
                println!("raw data: {}", data);
                serde_json::from_str(data.as_ref()).unwrap()
            })
            .and_then(move |data: HookData| {
                let app = app.clone();
                async move {
                    println!("hook: \n{:?}", data);
                    let command = data.command;
                    let session = data.session;
                    serde_json::to_string(
                        &session
                            .make_response(&match command {
                                HandlerCommand::ListGroups => {
                                    let app = app.lock().await;
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
                                    let mut app = app.lock().await;
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
                                    if let Some(lights) = app.lock().await.group(&name) {
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
                            .build(app.lock().await.lights()),
                    )
                    .map_err(|e| warp::reject::custom(SerdeRejection(e)))
                }
            });
        Compat::new(
            warp::serve(auth.or(fulfill).or(hook).or(warp::any().map(|| {
                println!("fallback");
                format!("lol")
            })))
            .run(([127, 0, 0, 1], 8080)),
        )
        .await;
    });
}
