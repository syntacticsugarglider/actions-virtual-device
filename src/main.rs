use std::{collections::HashMap, convert::Infallible, net::IpAddr, sync::Arc};

use async_compat::Compat;
use bytes::Bytes;
use futures::{pin_mut, StreamExt};
use lights::{tuya_scan, BroadlinkLight, EspLight, SengledLight};
use lights_broadlink::discover;
use lights_esp_strip::listen;
use lights_sengled::SengledApi;
use smol::{block_on, lock::Mutex};
use warp::Filter;

fn main() {
    block_on(async move {
        let app = Arc::new(Mutex::new(lights::App::new()));

        smol::spawn({
            let app = app.clone();
            async move {
                let stream = discover();
                pin_mut!(stream);
                while let Some(Ok(light)) = stream.next().await {
                    let mut light = light.connect().await.unwrap();
                    light.set_transition_duration(0).await.unwrap();
                    let mut app = app.lock().await;
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
                    let mut app = app.lock().await;
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
                        &lights::fulfill(data, &mut *app.lock().await).await,
                    ))
                }
            }
        });
        let hook = warp::path("hook").and(warp::body::json()).and_then({
            let app = app.clone();
            move |data| {
                let app = app.clone();
                async move { lights::hook(data, &mut *app.lock().await).await }
            }
        });

        let upload = warp::path!("upload" / String)
            .and(warp::body::bytes())
            .and_then({
                {
                    let esp_lights = esp_lights.clone();
                    move |id: String, binary: Bytes| {
                        let esp_lights = esp_lights.clone();
                        async move {
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

        let write = warp::path!("write" / String)
            .and(warp::body::bytes())
            .and_then({
                move |id: String, binary: Bytes| {
                    let esp_lights = esp_lights.clone();
                    async move {
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
                app.lock().await.push_lights(lights).await;
            }
        })
        .detach();

        let server = smol::spawn(Compat::new(
            warp::serve(lights::auth().or(fulfill).or(upload).or(write).or(hook))
                .run(([127, 0, 0, 1], 8080)),
        ));

        let sengled_api = Arc::new(
            SengledApi::new(
                std::env::var("SENGLED_USER").unwrap(),
                std::env::var("SENGLED_PASS").unwrap(),
            )
            .await
            .unwrap(),
        );

        for device in sengled_api.get_devices().await.unwrap() {
            app.lock()
                .await
                .push_light(SengledLight::new(device, sengled_api.clone()))
                .await;
        }

        server.await;
    });
}
