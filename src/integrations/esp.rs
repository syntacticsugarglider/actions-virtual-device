use crate::{Color, PowerState};
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

use lights_esp_strip::Light;

struct LightData {
    light: Light,
    brightness: u8,
    color: (u8, u8, u8),
}

pub struct EspLight {
    name: String,
    data: Mutex<LightData>,
}

impl crate::Light for EspLight {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn set_power_state<'a>(
        &'a self,
        state: crate::PowerState,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        let fut = match state {
            PowerState::On => {
                Either::Left(async move { self.data.lock().await.light.turn_on().await })
            }
            PowerState::Off => {
                Either::Right(async move { self.data.lock().await.light.turn_off().await })
            }
        };
        Box::pin(fut.map_err(|e| Box::new(e) as Box<dyn Error + Send>))
    }

    fn set_brightness<'a>(
        &'a self,
        brightness: u8,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        Box::pin(async move {
            let mut data = self.data.lock().await;
            let ratio = brightness as f32 / 255.;
            data.brightness = brightness;
            let color = (
                (data.color.0 as f32 * ratio) as u8,
                (data.color.1 as f32 * ratio) as u8,
                (data.color.2 as f32 * ratio) as u8,
            );
            data.light
                .set_color(color)
                .await
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)
        })
    }

    fn set_color<'a>(
        &'a self,
        color: crate::Color,
    ) -> BoxFuture<'a, Result<(), Box<dyn Error + Send>>> {
        Box::pin(async move {
            let mut data = self.data.lock().await;
            match color {
                Color::Rgb { r, g, b } => {
                    data.color = (r, g, b);
                }
                Color::White { temperature } => {
                    let mut color = (0, 0, 0);
                    let temperature = temperature as f64 / 100.;

                    color.0 = {
                        let mut red = 255.;
                        if temperature > 66. {
                            red = 329.698727466 * (temperature - 60.).powf(-0.1332047592);
                        }
                        red as u8
                    };

                    color.1 = {
                        (if temperature <= 66. {
                            (99.4708025861 * temperature.ln()) - 161.1195681661
                        } else {
                            288.1221695283 * (temperature - 60.).powf(-0.0755148492)
                        }) as u8
                    };

                    color.2 = {
                        let mut blue = 255.;
                        if temperature < 65. {
                            if temperature <= 19. {
                                blue = 0.;
                            } else {
                                blue = (138.5177312231 * (temperature - 10.).ln()) - 305.0447927307;
                            }
                        }
                        blue as u8
                    };

                    data.color = color;
                }
            };
            let brightness = data.brightness;
            drop(data);
            self.set_brightness(brightness).await
        })
    }

    fn unique_id<'a>(&'a self) -> BoxFuture<'a, Result<String, Box<dyn Error + Send>>> {
        Box::pin(async move {
            let data = self.data.lock().await;
            data.light
                .addr()
                .map(|addr| format!("Esp Light {}", addr))
                .map_err(|e| Box::new(e) as Box<dyn Error + Send>)
        })
    }
}

impl EspLight {
    pub fn new(light: Light) -> Self {
        EspLight {
            name: format!("ESP Light {}", COUNT.fetch_add(1, Ordering::SeqCst)),
            data: Mutex::new(LightData {
                light,
                color: (255, 255, 255),
                brightness: 255,
            }),
        }
    }
}
