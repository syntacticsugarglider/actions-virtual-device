use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Enumerate,
    CheckAuth,
    MakeGroup { lights: Vec<String>, id: String },
    AddLightToGroup { light: String, group: String },
    RemoveLightFromGroup { light: String, group: String },
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EnumerateResponse {
    pub lights: Vec<Light>,
    pub groups: Vec<Group>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Group {
    pub name: String,
    pub lights: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum State {
    Off,
    Rgb { red: u8, green: u8, blue: u8 },
    White { temp: u32 },
}

#[derive(Deserialize, Serialize, Debug)]
pub struct Light {
    pub id: String,
    pub state: State,
}

pub struct Enumerate;

impl IntoRequest for Enumerate {
    type Response = EnumerateResponse;

    fn into_request(self) -> Request {
        Request::Enumerate
    }
}

pub struct AddLightToGroup {
    pub light: String,
    pub group: String,
}

#[derive(Serialize, Deserialize)]
pub struct AddLightToGroupResponse;

impl IntoRequest for AddLightToGroup {
    type Response = AddLightToGroupResponse;

    fn into_request(self) -> Request {
        Request::AddLightToGroup {
            light: self.light,
            group: self.group,
        }
    }
}

pub struct RemoveLightFromGroup {
    pub light: String,
    pub group: String,
}

#[derive(Serialize, Deserialize)]
pub struct RemoveLightFromGroupResponse;

impl IntoRequest for RemoveLightFromGroup {
    type Response = RemoveLightFromGroupResponse;

    fn into_request(self) -> Request {
        Request::RemoveLightFromGroup {
            light: self.light,
            group: self.group,
        }
    }
}

pub struct MakeGroup {
    pub lights: Vec<String>,
    pub id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MakeGroupResponse;

impl IntoRequest for MakeGroup {
    type Response = MakeGroupResponse;

    fn into_request(self) -> Request {
        Request::MakeGroup {
            lights: self.lights,
            id: self.id,
        }
    }
}

#[derive(Deserialize, Serialize, Debug)]
pub struct CheckAuthResponse;

pub struct CheckAuth;

impl IntoRequest for CheckAuth {
    type Response = CheckAuthResponse;

    fn into_request(self) -> Request {
        Request::CheckAuth
    }
}

pub trait IntoRequest {
    type Response: for<'de> Deserialize<'de>;

    fn into_request(self) -> Request;
}

pub async fn request<T: IntoRequest>(key: &str, request: T) -> Result<T::Response, surf::Error> {
    surf::post(format!(
        "https://lightsmanager.syntacticsugarglider.com/api/{}",
        key
    ))
    .body(surf::Body::from_json(&request.into_request())?)
    .recv_json()
    .await
}
