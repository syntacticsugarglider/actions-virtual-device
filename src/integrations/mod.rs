use std::sync::Arc;

use crate::Light;

pub mod broadlink;
pub mod esp;
pub mod sengled;
pub mod tuya;

impl<T: Light> Light for Arc<T> {
    fn name(&self) -> String {
        T::name(self)
    }

    fn unique_id<'a>(
        &'a self,
    ) -> futures::future::BoxFuture<'a, Result<String, Box<dyn std::error::Error + Send>>> {
        T::unique_id(self)
    }

    fn set_power_state<'a>(
        &'a self,
        state: crate::PowerState,
    ) -> futures::future::BoxFuture<'a, Result<(), Box<dyn std::error::Error + Send>>> {
        T::set_power_state(self, state)
    }

    fn set_brightness<'a>(
        &'a self,
        brightness: u8,
    ) -> futures::future::BoxFuture<'a, Result<(), Box<dyn std::error::Error + Send>>> {
        T::set_brightness(self, brightness)
    }

    fn set_color<'a>(
        &'a self,
        color: crate::Color,
    ) -> futures::future::BoxFuture<'a, Result<(), Box<dyn std::error::Error + Send>>> {
        T::set_color(self, color)
    }
}
