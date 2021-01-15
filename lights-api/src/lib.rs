use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum Request {
    Enumerate,
    CheckAuth,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EnumerateResponse {
    pub lights: Vec<Light>,
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
