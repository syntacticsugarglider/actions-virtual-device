use std::{
    borrow::Borrow,
    collections::HashMap,
    error::Error as StdError,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc,
    },
};

mod auth;
pub use auth::auth;
mod fulfill;
mod hook;
pub use fulfill::fulfill;
pub use hook::hook;
mod util;
use util::format_list;
use uuid::Uuid;
mod request_sync;
use futures::future::BoxFuture;
use request_sync::request_sync;
use thiserror::Error;

mod integrations;
pub use integrations::broadlink::BroadlinkLight;
pub use integrations::sengled::SengledLight;
pub use integrations::tuya::{tuya_scan, TuyaLight};

pub enum PowerState {
    On,
    Off,
}

#[derive(Clone, Copy)]
pub enum Color {
    Rgb { r: u8, g: u8, b: u8 },
    White,
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
}

impl AtomicColor {
    fn new() -> Self {
        AtomicColor {
            white: AtomicBool::new(true),
            red: AtomicU8::new(255),
            green: AtomicU8::new(255),
            blue: AtomicU8::new(255),
        }
    }
    fn store(&self, color: Color, ordering: Ordering) {
        match color {
            Color::White => self.white.store(true, ordering),
            Color::Rgb { r, g, b } => {
                self.red.store(r, ordering);
                self.green.store(g, ordering);
                self.blue.store(b, ordering);
            }
        }
    }
    fn load(&self, ordering: Ordering) -> Color {
        if self.white.load(ordering) {
            Color::White
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

pub struct App {
    by_id: HashMap<Uuid, Arc<LightWrapper>>,
    by_name: HashMap<String, Arc<LightWrapper>>,
    groups: HashMap<String, Vec<Arc<LightWrapper>>>,
}

struct LightWrapper {
    light: Box<dyn Light + Sync + Send>,
    id: Uuid,
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
        self.id.to_string()
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
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("light error: {0}")]
    Light(#[from] Box<dyn StdError + Send>),
    #[error("invalid UUID: {0}")]
    Uuid(#[from] uuid::Error),
    #[error("nonexistent light accessed")]
    Absent,
}

impl App {
    pub fn new() -> App {
        App {
            by_name: HashMap::new(),
            groups: HashMap::new(),
            by_id: HashMap::new(),
        }
    }
    pub fn push_light<T: Light + Sync + Send + 'static>(&mut self, light: T) {
        let id = Uuid::new_v4();
        let light = Arc::new(LightWrapper {
            id,
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
    pub fn push_lights<I: IntoIterator<Item = T>, T: Light + Sync + Send + 'static>(
        &mut self,
        lights: I,
    ) {
        for light in lights {
            let id = Uuid::new_v4();
            let light = Arc::new(LightWrapper {
                id,
                brightness: AtomicU8::new(0),
                is_on: AtomicBool::new(false),
                color: AtomicColor::new(),
                light: Box::new(light),
            });
            self.by_id.insert(id, light.clone());
        }
        smol::spawn(async move {
            if let Err(e) = request_sync().await {
                eprintln!("sync request failed: {:?}", e);
            }
        })
        .detach();
    }
    fn groups(&self) -> impl ExactSizeIterator<Item = &String> {
        self.groups.keys()
    }
    fn light_names<'a>(&'a self) -> impl ExactSizeIterator<Item = String> + 'a {
        self.by_id.values().map(|light| light.name())
    }
    fn lights(&self) -> impl ExactSizeIterator<Item = &LightWrapper> {
        self.by_id.values().map(|light| light.as_ref())
    }
    async fn set_state(&mut self, id: &str, state: PowerState) -> Result<(), Error> {
        let wrapper = self.by_id.get(&Uuid::parse_str(id)?).ok_or(Error::Absent)?;
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
        let wrapper = self.by_id.get(&Uuid::parse_str(id)?).ok_or(Error::Absent)?;
        wrapper.brightness.store(brightness, Ordering::SeqCst);
        wrapper.light().set_brightness(brightness).await?;
        Ok(())
    }
    async fn set_color(&mut self, id: &str, color: Color) -> Result<(), Error> {
        let wrapper = self.by_id.get(&Uuid::parse_str(id)?).ok_or(Error::Absent)?;
        wrapper.color.store(color, Ordering::SeqCst);
        wrapper.light().set_color(color).await?;
        Ok(())
    }
    fn add_lights(&mut self, group: &str, lights: impl IntoIterator<Item = String>) {
        let lights = lights
            .into_iter()
            .map(|light| self.by_name.get(&light).unwrap().clone())
            .collect::<Vec<_>>();
        self.groups.get_mut(group).unwrap().extend(lights)
    }
    fn group(&self, group: &str) -> Option<&[Arc<LightWrapper>]> {
        self.groups.get(group).map(|a| a.as_slice())
    }
    fn has_group(&self, group: &str) -> bool {
        self.groups.contains_key(group)
    }
    fn absent<'a, T: Borrow<String>, I: Iterator<Item = T> + 'a>(
        &'a self,
        lights: I,
    ) -> impl Iterator<Item = T> + 'a {
        lights.filter(move |light| !self.by_name.contains_key(light.borrow()))
    }
}
