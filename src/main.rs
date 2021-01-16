use std::{collections::HashMap, convert::Infallible, io::Read, net::IpAddr, sync::Arc};

use async_compat::Compat;
use bytes::Bytes;
use futures::{pin_mut, StreamExt};
use lights::{tuya_scan, BroadlinkLight, EspLight};
use lights_broadlink::discover;
use lights_esp_strip::listen;
use smol::{
    block_on,
    lock::{Mutex, RwLock},
};
use warp::Filter;

const AUTH_TOKEN: &'static str = env!("ESP_AUTH_TOKEN");

fn main() {
    block_on(async move {
        let app = Arc::new(RwLock::new(lights::App::new()));

        smol::spawn({
            let app = app.clone();
            async move {
                let stream = discover();
                pin_mut!(stream);
                while let Some(Ok(light)) = stream.next().await {
                    let mut light = light.connect().await.unwrap();
                    light.set_transition_duration(0).await.unwrap();
                    let mut app = app.write().await;
                    app.push_light(BroadlinkLight::new(light)).await;
                }
            }
        })
        .detach();

        let esp_lights = Arc::new(Mutex::new(HashMap::new()));

        smol::spawn({
            let app = app.clone();
            let esp_lights = esp_lights.clone();
            async move {
                let stream = listen(5000);
                pin_mut!(stream);
                while let Some(Ok(light)) = stream.next().await {
                    let mut app = app.write().await;
                    let light = Arc::new(EspLight::new(light));
                    app.push_light(light.clone()).await;
                    esp_lights
                        .lock()
                        .await
                        .insert(light.addr().await.unwrap(), light);
                }
            }
        })
        .detach();

        let fulfill = warp::path("fulfill").and(warp::body::json()).and_then({
            let app = app.clone();
            move |data| {
                let app = app.clone();
                async move {
                    Ok::<_, Infallible>(warp::reply::json(
                        &lights::fulfill(data, &*app.read().await).await,
                    ))
                }
            }
        });

        let upload = warp::path!("upload" / String / String)
            .and(warp::body::bytes())
            .and_then({
                {
                    let esp_lights = esp_lights.clone();
                    move |token: String, id: String, binary: Bytes| {
                        let esp_lights = esp_lights.clone();
                        async move {
                            if token != AUTH_TOKEN {
                                return Ok::<_, Infallible>(format!(""));
                            }
                            let binary: &[u8] = binary.as_ref();
                            let addr: Result<IpAddr, _> = id.parse();
                            if let Ok(addr) = addr {
                                if let Some(light) = esp_lights.lock().await.get(&addr) {
                                    light.try_program(&binary).await;
                                }
                            }
                            Ok::<_, Infallible>(format!(""))
                        }
                    }
                }
            });

        let write = warp::path!("write" / String / String)
            .and(warp::body::bytes())
            .and_then({
                move |token: String, id: String, binary: Bytes| {
                    let esp_lights = esp_lights.clone();
                    async move {
                        if token != AUTH_TOKEN {
                            return Ok::<_, Infallible>(format!(""));
                        }
                        let binary: &[u8] = binary.as_ref();
                        let addr: Result<IpAddr, _> = id.parse();
                        if let Ok(addr) = addr {
                            if let Some(light) = esp_lights.lock().await.get(&addr) {
                                light.try_write(&binary).await;
                            }
                        }
                        Ok::<_, Infallible>(format!(""))
                    }
                }
            });

        smol::spawn({
            let app = app.clone();
            async move {
                let lights = tuya_scan(
                    std::env::var("TUYA_USER").unwrap(),
                    std::env::var("TUYA_PASS").unwrap(),
                )
                .await
                .unwrap();
                app.write().await.push_lights(lights).await;
            }
        })
        .detach();

        let server = smol::spawn(Compat::new(
            warp::serve(
                lights::api(app.clone())
                    .or(lights::auth())
                    .or(fulfill)
                    .or(upload)
                    .or(write)
                    .or(warp::path::end().map(|| {
                        let mut string = String::new();
                        std::fs::File::open("ui.html")
                            .unwrap()
                            .read_to_string(&mut string)
                            .unwrap();
                        warp::reply::html(string)
                    })),
            )
            .run(([127, 0, 0, 1], 8080)),
        ));

        server.await;
    });
}
