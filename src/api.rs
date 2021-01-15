use std::sync::Arc;

use lights_api::{Light, Request, State};
use smol::lock::Mutex;
use warp::{filters::BoxedFilter, Filter, Reply};

use crate::{App, Color};

pub fn api(app: Arc<Mutex<App>>) -> BoxedFilter<(impl Reply,)> {
    let api = warp::path!("api" / String)
        .and(warp::body::json())
        .and_then(move |token, request: Request| {
            let app = app.clone();
            async move {
                Ok::<_, core::convert::Infallible>(if token == env!("API_AUTH_TOKEN") {
                    match request {
                        Request::Enumerate => warp::reply::json({
                            let app = app.lock().await;
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
                            &lights_api::EnumerateResponse { lights }
                        }),
                        Request::CheckAuth => warp::reply::json(&lights_api::CheckAuthResponse),
                    }
                } else {
                    warp::reply::json(&format!("bad auth"))
                })
            }
        });
    api.boxed()
}
