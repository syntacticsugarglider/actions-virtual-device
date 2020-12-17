use std::{convert::Infallible, sync::Arc};

use async_compat::Compat;
use futures::{pin_mut, StreamExt};
use lights_broadlink::discover;
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
                    light.turn_on().await.unwrap();
                    let mut app = app.lock().await;
                    app.push_light(Mutex::new(light));
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
        let hook = warp::path("hook")
            .and(warp::body::json())
            .and_then(move |data| {
                let app = app.clone();
                async move { lights::hook(data, &mut *app.lock().await).await }
            });
        Compat::new(warp::serve(lights::auth().or(fulfill).or(hook)).run(([127, 0, 0, 1], 8080)))
            .await;
    });
}
