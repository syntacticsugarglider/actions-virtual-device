use crate::PowerState;
use futures::{
    future::{BoxFuture, Either},
    TryFutureExt,
};
use lights_sengled::{Color, Device, SengledApi};
use std::{
    error::Error,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

static COUNT: AtomicUsize = AtomicUsize::new(1);

pub struct SengledLight {
    api: Arc<SengledApi>,
    name: String,
    light: Device,
}

impl crate::Light for SengledLight {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn set_power_state<'a>(
        &'a self,
        state: crate::PowerState,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        let fut = match state {
            PowerState::On => Either::Left(async move { self.api.turn_on(&self.light).await }),
            PowerState::Off => Either::Right(async move { self.api.turn_off(&self.light).await }),
        };
        Box::pin(fut.map_err(|e| Box::new(e) as Box<dyn Error + Send>))
    }

    fn set_brightness<'a>(
        &'a self,
        brightness: u8,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        Box::pin(async move {
            self.api
                .set_brightness(&self.light, brightness)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)
        })
    }

    fn set_color<'a>(
        &'a self,
        color: crate::Color,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        Box::pin(async move {
            self.api
                .set_color(
                    &self.light,
                    match color {
                        crate::Color::Rgb { r, g, b } => Color::Rgb {
                            red: r,
                            green: g,
                            blue: b,
                        },
                        crate::Color::White { temperature } => Color::White { temperature },
                    },
                )
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)
        })
    }

    fn unique_id<'a>(&'a self) -> BoxFuture<'a, Result<String, Box<dyn Error + Send>>> {
        Box::pin(async move { Ok(format!("Sengled Light {:?}", self.light.uuid())) })
    }
}

impl SengledLight {
    pub fn new(light: Device, api: Arc<SengledApi>) -> Self {
        SengledLight {
            name: format!("Sengled Light {}", COUNT.fetch_add(1, Ordering::SeqCst)),
            light,
            api,
        }
    }
}
