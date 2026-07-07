mod config;
mod minio;

use std::collections::HashMap;

use axum::{
    extract::{Request, State},
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use axum_auth::AuthBasic;
use dav_server::fakels::FakeLs;
use dav_server::DavHandler;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
struct AppState {
    dav: DavHandler,
    accounts: HashMap<String, String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let conf = config::Config::load();

    let fs = minio::MinioFs::new(
        &conf.minio.endpoint,
        &conf.minio.bucket_name,
        conf.minio.ssl,
        &conf.minio.access_key,
        &conf.minio.secret_access_key,
        &conf.app.upload_mode,
    )
    .await;

    let dav = DavHandler::builder()
        .filesystem(Box::new(fs))
        .locksystem(FakeLs::new())
        .strip_prefix("/")
        .build_handler();

    let mut accounts: HashMap<String, String> = HashMap::new();
    for account in &conf.app.accounts {
        tracing::info!("Auth account: {}:******", account.username);
        accounts.insert(account.username.clone(), account.password.clone());
    }

    let state = AppState { dav, accounts };

    let app = Router::new()
        .route("/", any(handle_dav))
        .route("/{*path}", any(handle_dav))
        .with_state(state)
        .layer(TraceLayer::new_for_http());

    let addr = format!("0.0.0.0:{}", conf.app.port);
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handle_dav(State(state): State<AppState>, auth: AuthBasic, mut req: Request) -> Response {
    let AuthBasic((user, pass)) = auth;

    let authorized = state
        .accounts
        .get(&user)
        .map(|expected| pass.as_deref() == Some(expected))
        .unwrap_or(false);

    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [("WWW-Authenticate", "Basic realm=\"mindav\"")],
        )
            .into_response();
    }

    let path = req.uri().path().to_string();
    let new_path = format!("/{}{}", user, path);

    let new_path_and_query = if let Some(query) = req.uri().query() {
        format!("{}?{}", new_path, query)
    } else {
        new_path
    };
    let mut parts = req.uri().clone().into_parts();
    parts.path_and_query = Some(new_path_and_query.parse().unwrap());
    *req.uri_mut() = Uri::from_parts(parts).unwrap();

    let resp = state.dav.handle(req).await;
    let (parts, body) = resp.into_parts();
    let body = axum::body::Body::new(body);
    Response::from_parts(parts, body)
}
