use crate::PowerState;
use futures::{
    future::{BoxFuture, Either},
    TryFutureExt,
};
use lights_tuya::{AccessToken, Light, State, TuyaApi};
use serde::{Deserialize, Serialize};
use std::{
    error::Error,
    io::{Read, Write},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

static COUNT: AtomicUsize = AtomicUsize::new(1);

#[derive(Serialize, Deserialize)]
struct DevicesFile {
    devices: Vec<Light>,
}

pub struct TuyaLight {
    api: Arc<TuyaApi>,
    name: String,
    light: Light,
}

impl crate::Light for TuyaLight {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn set_power_state<'a>(
        &'a self,
        state: crate::PowerState,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        let fut = match state {
            PowerState::On => {
                Either::Left(async move { self.api.set_state(&self.light, State::On).await })
            }
            PowerState::Off => {
                Either::Right(async move { self.api.set_state(&self.light, State::Off).await })
            }
        };
        Box::pin(fut.map_err(|e| Box::new(e) as Box<dyn Error + Send>))
    }

    fn set_brightness<'a>(
        &'a self,
        brightness: u8,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        Box::pin(async move {
            self.api
                .set_brightness(&self.light, brightness as u16)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)
        })
    }
}

impl TuyaLight {
    pub fn new(light: Light, api: Arc<TuyaApi>) -> Self {
        TuyaLight {
            name: format!("Tuya Light {}", COUNT.fetch_add(1, Ordering::SeqCst)),
            light,
            api,
        }
    }
}

pub async fn tuya_scan<T: AsRef<str>, U: AsRef<str>>(
    user: T,
    pass: U,
) -> Result<Vec<TuyaLight>, Box<dyn Error>> {
    let key_path = std::path::Path::new("tuya/access_token");
    let api = Arc::new(if key_path.exists() {
        let file = std::fs::OpenOptions::new().read(true).open(key_path)?;
        TuyaApi::from_token(AccessToken::read_from(file)?)
    } else {
        let api = TuyaApi::new(user, pass).await?;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(key_path)?;
        api.dump_token().write_to(file)?;
        api
    });
    let devices_path = std::path::Path::new("tuya/devices.toml");
    Ok(if devices_path.exists() {
        let mut buf = String::new();
        std::fs::File::open(devices_path)?.read_to_string(&mut buf)?;
        let DevicesFile { devices } = toml::from_str(&buf)?;
        devices
    } else {
        let devices = api.scan().await?;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(devices_path)?
            .write_all(
                toml::to_string(&DevicesFile {
                    devices: devices.clone(),
                })?
                .as_bytes(),
            )?;
        devices
    }
    .into_iter()
    .map(|light| TuyaLight::new(light, api.clone()))
    .collect())
}
