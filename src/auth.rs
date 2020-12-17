use http::Uri;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use warp::{filters::BoxedFilter, Filter, Reply};

#[derive(Deserialize, Debug)]
struct TokenQuery {
    client_id: String,
    client_secret: String,
    grant_type: Option<String>,
    code: Option<String>,
    redirect_uri: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Serialize)]
struct TokenResponse {
    token_type: String,
    access_token: String,
    refresh_token: String,
    expires_in: u32,
}

#[derive(Deserialize, Debug)]
struct OauthQuery {
    client_id: String,
    redirect_uri: String,
    state: String,
    response_type: String,
    user_locale: String,
    scope: Option<String>,
}

pub fn auth() -> BoxedFilter<(impl Reply,)> {
    let authorization_code = uuid::Uuid::new_v4().to_string();
    let access_token = authorization_code.clone();
    let refresh_token = access_token.clone();

    let auth = warp::path("auth");
    let auth_init =
        auth.and(warp::path("auth"))
            .and(warp::query())
            .map(move |query: OauthQuery| {
                println!("auth");
                warp::redirect::redirect(
                    Uri::try_from(format!(
                        "{}?code={}&state={}",
                        query.redirect_uri, authorization_code, query.state
                    ))
                    .unwrap(),
                )
            });
    let auth_token =
        auth.and(warp::path("token"))
            .and(warp::body::form())
            .map(move |query: TokenQuery| {
                println!("token req: {:?}", query);
                warp::reply::json(&TokenResponse {
                    token_type: "Bearer".to_owned(),
                    access_token: access_token.clone(),
                    refresh_token: refresh_token.clone(),
                    expires_in: 360,
                })
            });
    let auth = auth_init.or(auth_token);
    auth.boxed()
}
