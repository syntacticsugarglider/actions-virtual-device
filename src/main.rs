use std::{convert::Infallible, sync::Arc};

use async_compat::Compat;
use futures::{pin_mut, StreamExt};
use lights::{tuya_scan, BroadlinkLight, SengledLight};
use lights_broadlink::discover;
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
                    app.push_light(BroadlinkLight::new(light));
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

        smol::spawn({
            let app = app.clone();
            async move {
                let lights = tuya_scan(
                    std::env::var("TUYA_USER").unwrap(),
                    std::env::var("TUYA_PASS").unwrap(),
                )
                .await
                .unwrap();
                app.lock().await.push_lights(lights);
            }
        })
        .detach();

        let server = smol::spawn(Compat::new(
            warp::serve(lights::auth().or(fulfill).or(hook)).run(([127, 0, 0, 1], 8080)),
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
                .push_light(SengledLight::new(device, sengled_api.clone()));
        }

        server.await;
    });
}
