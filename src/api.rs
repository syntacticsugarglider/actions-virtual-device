use std::{collections::HashMap, sync::Arc};

use futures::{stream::iter, StreamExt};
use lazy_static::lazy_static;
use lights_api::{Light, Request, State};
use smol::lock::{Mutex, RwLock};
use warp::{filters::BoxedFilter, Filter, Reply};

use crate::{App, Color};

lazy_static! {
    static ref GROUPS: Mutex<HashMap<String, Arc<Group>>> = Mutex::new(HashMap::new());
}

pub fn api(app: Arc<RwLock<App>>) -> BoxedFilter<(impl Reply,)> {
    let api = warp::path!("api" / String)
        .and(warp::body::json())
        .and_then(move |token, request: Request| {
            let app = app.clone();
            async move {
                Ok::<_, core::convert::Infallible>(if token == env!("API_AUTH_TOKEN") {
                    match request {
                        Request::Enumerate => warp::reply::json({
                            let app = app.read().await;
                            let lights = app
                                .lights()
                                .map(|item| Light {
                                    id: item.id(),
                                    state: if item.is_on() {
                                        match item.rgb_color() {
                                            Color::White { temperature } => {
                                                State::White { temp: temperature }
                                            }
                                            Color::Rgb { r, g, b } => State::Rgb {
                                                red: r,
                                                green: g,
                                                blue: b,
                                            },
                                        }
                                    } else {
                                        State::Off
                                    },
                                })
                                .collect();
                            &lights_api::EnumerateResponse {
                                lights,
                                groups: iter(GROUPS.lock().await.iter())
                                    .then(|(id, group)| async move {
                                        lights_api::Group {
                                            name: format!("Group {}", id),
                                            lights: group.lights.lock().await.clone(),
                                        }
                                    })
                                    .collect()
                                    .await,
                            }
                        }),
                        Request::CheckAuth => warp::reply::json(&lights_api::CheckAuthResponse),
                        Request::MakeGroup { lights, id } => {
                            let group = Arc::new(Group {
                                name: format!("Group {}", id),
                                lights: Mutex::new(lights),
                                app: app.clone(),
                                id: id.clone(),
                            });
                            app.write().await.push_light(group.clone()).await;
                            GROUPS.lock().await.insert(id.clone(), group);
                            warp::reply::json(&lights_api::MakeGroupResponse)
                        }
                        Request::AddLightToGroup { light, group } => {
                            GROUPS
                                .lock()
                                .await
                                .get(&group)
                                .unwrap()
                                .lights
                                .lock()
                                .await
                                .push(light);
                            warp::reply::json(&lights_api::AddLightToGroupResponse)
                        }
                        Request::RemoveLightFromGroup { light, group } => {
                            let lights = GROUPS.lock().await;
                            let mut lights = lights.get(&group).unwrap().lights.lock().await;
                            let idx = lights.iter().position(|item| item == &light).unwrap();
                            lights.remove(idx);
                            warp::reply::json(&lights_api::RemoveLightFromGroupResponse)
                        }
                    }
                } else {
                    warp::reply::json(&format!("bad auth"))
                })
            }
        });
    api.boxed()
}

pub struct Group {
    name: String,
    lights: Mutex<Vec<String>>,
    id: String,
    app: Arc<RwLock<App>>,
}

impl crate::Light for Group {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn unique_id<'a>(
        &'a self,
    ) -> futures::future::BoxFuture<'a, Result<String, Box<dyn std::error::Error + Send>>> {
        Box::pin(async move { Ok(format!("Group {}", self.id)) })
    }

    fn set_power_state<'a>(
        &'a self,
        state: crate::PowerState,
    ) -> futures::future::BoxFuture<'a, Result<(), Box<dyn std::error::Error + Send>>> {
        Box::pin(async move {
            let app = self.app.read().await;
            for light in &*self.lights.lock().await {
                app.set_state(&*light, state)
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            }
            Ok(())
        })
    }

    fn set_brightness<'a>(
        &'a self,
        brightness: u8,
    ) -> futures::future::BoxFuture<'a, Result<(), Box<dyn std::error::Error + Send>>> {
        Box::pin(async move {
            let app = self.app.read().await;
            for light in &*self.lights.lock().await {
                app.set_brightness(&*light, brightness)
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            }
            Ok(())
        })
    }

    fn set_color<'a>(
        &'a self,
        color: Color,
    ) -> futures::future::BoxFuture<'a, Result<(), Box<dyn std::error::Error + Send>>> {
        Box::pin(async move {
            let app = self.app.read().await;
            for light in &*self.lights.lock().await {
                app.set_color(&*light, color)
                    .await
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error + Send>)?;
            }
            Ok(())
        })
    }
}
