use serde::Serialize;
use surf::Body;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncRequestBody {
    agent_user_id: &'static str,
}

pub async fn request_sync() -> Result<(), surf::Error> {
    surf::post("https://homegraph.googleapis.com/v1/devices:requestSync")
        .header(
            "Authorization",
            format!("Bearer {}", std::env::var("HOME_GRAPH_TOKEN").unwrap()),
        )
        .body(Body::from_json(&SyncRequestBody {
            agent_user_id: "haha.yes",
        })?)
        .await?;
    Ok(())
}
