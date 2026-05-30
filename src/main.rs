mod config;
mod minio;
mod utils;

use axum::{
    extract::{Request, State},
    http::StatusCode,
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
    admin_name: String,
    admin_password: String,
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

    tracing::info!("Auth accounts: {}:******", conf.app.admin.username);

    let state = AppState {
        dav,
        admin_name: conf.app.admin.username.clone(),
        admin_password: conf.app.admin.password.clone(),
    };

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

async fn handle_dav(
    State(state): State<AppState>,
    auth: AuthBasic,
    req: Request,
) -> Response {
    let AuthBasic((user, pass)) = auth;
    if user == state.admin_name && pass.as_deref() == Some(&state.admin_password) {
        let resp = state.dav.handle(req).await;
        let (parts, body) = resp.into_parts();
        let body = axum::body::Body::new(body);
        Response::from_parts(parts, body)
    } else {
        (StatusCode::UNAUTHORIZED, [("WWW-Authenticate", "Basic realm=\"mindav\"")]).into_response()
    }
}
