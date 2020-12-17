use crate::PowerState;
use futures::{
    future::{BoxFuture, Either},
    TryFutureExt,
};
use smol::lock::Mutex;
use std::{
    error::Error,
    sync::atomic::{AtomicUsize, Ordering},
};

static COUNT: AtomicUsize = AtomicUsize::new(1);

use lights_broadlink::Connection;

pub struct BroadlinkLight {
    name: String,
    light: Mutex<Connection>,
}

impl crate::Light for BroadlinkLight {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn set_power_state<'a>(
        &'a self,
        state: crate::PowerState,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        let fut = match state {
            PowerState::On => Either::Left(async move { self.light.lock().await.turn_on().await }),
            PowerState::Off => {
                Either::Right(async move { self.light.lock().await.turn_off().await })
            }
        };
        Box::pin(fut.map_err(|e| Box::new(e) as Box<dyn Error + Send>))
    }
}

impl BroadlinkLight {
    pub fn new(light: Connection) -> Self {
        BroadlinkLight {
            name: format!("Aliexpress Light {}", COUNT.fetch_add(1, Ordering::SeqCst)),
            light: Mutex::new(light),
        }
    }
}
