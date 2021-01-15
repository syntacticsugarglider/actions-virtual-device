use std::{
    collections::HashMap,
    error::Error as StdError,
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering},
        Arc,
    },
};

mod auth;
pub use auth::auth;
mod fulfill;
pub use fulfill::fulfill;
mod request_sync;
use futures::future::BoxFuture;
use request_sync::request_sync;
use thiserror::Error;
mod api;
pub use api::api;

mod integrations;
pub use integrations::broadlink::BroadlinkLight;
pub use integrations::esp::EspLight;
// pub use integrations::sengled::SengledLight;
pub use integrations::tuya::{tuya_scan, TuyaLight};

pub enum PowerState {
    On,
    Off,
}

#[derive(Clone, Copy)]
pub enum Color {
    Rgb { r: u8, g: u8, b: u8 },
    White { temperature: u32 },
}

impl Color {
    pub(crate) fn to_spectrum(&self) -> u32 {
        let (r, g, b) = match self {
            Color::Rgb { r, g, b } => (*r, *g, *b),
            _ => (255, 255, 255),
        };
        u32::from_str_radix(&format!("{:02X}{:02X}{:02X}", r, g, b), 16).unwrap()
    }
}

impl From<bool> for PowerState {
    fn from(data: bool) -> Self {
        match data {
            true => PowerState::On,
            false => PowerState::Off,
        }
    }
}

struct AtomicColor {
    red: AtomicU8,
    blue: AtomicU8,
    green: AtomicU8,
    white: AtomicBool,
    temperature: AtomicU32,
}

impl AtomicColor {
    fn new() -> Self {
        AtomicColor {
            white: AtomicBool::new(true),
            red: AtomicU8::new(255),
            green: AtomicU8::new(255),
            blue: AtomicU8::new(255),
            temperature: AtomicU32::new(65000),
        }
    }
    fn store(&self, color: Color, ordering: Ordering) {
        match color {
            Color::White { temperature } => {
                self.temperature.store(temperature, ordering);
                self.white.store(true, ordering)
            }
            Color::Rgb { r, g, b } => {
                self.white.store(false, ordering);
                self.red.store(r, ordering);
                self.green.store(g, ordering);
                self.blue.store(b, ordering);
            }
        }
    }
    fn load(&self, ordering: Ordering) -> Color {
        if self.white.load(ordering) {
            Color::White {
                temperature: self.temperature.load(ordering),
            }
        } else {
            Color::Rgb {
                r: self.red.load(ordering),
                g: self.green.load(ordering),
                b: self.blue.load(ordering),
            }
        }
    }
}

pub trait Light {
    fn name(&self) -> String;

    fn unique_id<'a>(&'a self) -> BoxFuture<'a, Result<String, Box<dyn StdError + Send>>>;

    fn set_power_state<'a>(
        &'a self,
        state: PowerState,
    ) -> BoxFuture<'a, Result<(), Box<dyn StdError + Send>>>;

    fn set_brightness<'a>(
        &'a self,
        brightness: u8,
    ) -> BoxFuture<'a, Result<(), Box<dyn StdError + Send>>>;

    fn set_color<'a>(&'a self, color: Color)
        -> BoxFuture<'a, Result<(), Box<dyn StdError + Send>>>;
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct Id(String);

pub struct App {
    by_id: HashMap<Id, Arc<LightWrapper>>,
}

struct LightWrapper {
    light: Box<dyn Light + Sync + Send>,
    id: Id,
    brightness: AtomicU8,
    is_on: AtomicBool,
    color: AtomicColor,
}

impl LightWrapper {
    fn name(&self) -> String {
        self.light.name()
    }
    fn brightness(&self) -> u8 {
        self.brightness.load(Ordering::SeqCst)
    }
    fn id(&self) -> String {
        self.id.0.clone()
    }
    fn light(&self) -> &(dyn Light + Sync + Send) {
        self.light.as_ref()
    }
    fn is_on(&self) -> bool {
        self.is_on.load(Ordering::SeqCst)
    }
    fn color(&self) -> u32 {
        self.color.load(Ordering::SeqCst).to_spectrum()
    }
    fn rgb_color(&self) -> Color {
        self.color.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("light error: {0}")]
    Light(#[from] Box<dyn StdError + Send>),
    #[error("nonexistent light accessed")]
    Absent,
}

impl App {
    pub fn new() -> App {
        App {
            by_id: HashMap::new(),
        }
    }
    pub async fn push_light<T: Light + Sync + Send + 'static>(&mut self, light: T) {
        if let Ok(id) = light.unique_id().await {
            let id = Id(id);
            let light = Arc::new(LightWrapper {
                id: id.clone(),
                light: Box::new(light),
                brightness: AtomicU8::new(0),
                color: AtomicColor::new(),
                is_on: AtomicBool::new(false),
            });
            self.by_id.insert(id, light.clone());
            smol::spawn(async move {
                if let Err(e) = request_sync().await {
                    eprintln!("sync request failed: {:?}", e);
                }
            })
            .detach();
        }
    }
    pub async fn push_lights<I: IntoIterator<Item = T>, T: Light + Sync + Send + 'static>(
        &mut self,
        lights: I,
    ) {
        for light in lights {
            if let Ok(id) = light.unique_id().await {
                let id = Id(id);
                let light = Arc::new(LightWrapper {
                    id: id.clone(),
                    brightness: AtomicU8::new(0),
                    is_on: AtomicBool::new(false),
                    color: AtomicColor::new(),
                    light: Box::new(light),
                });
                self.by_id.insert(id, light.clone());
            }
        }
        smol::spawn(async move {
            if let Err(e) = request_sync().await {
                eprintln!("sync request failed: {:?}", e);
            }
        })
        .detach();
    }
    fn lights(&self) -> impl ExactSizeIterator<Item = &LightWrapper> {
        self.by_id.values().map(|light| light.as_ref())
    }
    async fn set_state(&mut self, id: &str, state: PowerState) -> Result<(), Error> {
        let wrapper = self.by_id.get(&Id(id.into())).ok_or(Error::Absent)?;
        wrapper.is_on.store(
            match state {
                PowerState::On => true,
                PowerState::Off => false,
            },
            Ordering::SeqCst,
        );
        wrapper.light().set_power_state(state).await?;
        Ok(())
    }
    async fn set_brightness(&mut self, id: &str, brightness: u8) -> Result<(), Error> {
        let wrapper = self.by_id.get(&Id(id.into())).ok_or(Error::Absent)?;
        wrapper.brightness.store(brightness, Ordering::SeqCst);
        wrapper.light().set_brightness(brightness).await?;
        Ok(())
    }
    async fn set_color(&mut self, id: &str, color: Color) -> Result<(), Error> {
        let wrapper = self.by_id.get(&Id(id.into())).ok_or(Error::Absent)?;
        wrapper.color.store(color, Ordering::SeqCst);
        wrapper.light().set_color(color).await?;
        Ok(())
    }
}
