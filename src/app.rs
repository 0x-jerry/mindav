use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use axum_auth::AuthBasic;
use dav_server::davpath::DavPath;
use dav_server::fakels::FakeLs;
use dav_server::fs::DavFileSystem;
use dav_server::DavHandler;
use tower_http::trace::TraceLayer;

use crate::userfs::UserScopedFs;

#[derive(Clone)]
pub struct AppState<F> {
    fs: F,
    accounts: HashMap<String, String>,
    handlers: Arc<RwLock<HashMap<String, DavHandler>>>,
}

pub fn build_router<F>(fs: F, accounts: HashMap<String, String>) -> Router
where
    F: DavFileSystem + Clone + Send + Sync + 'static,
{
    let state = AppState {
        fs,
        accounts,
        handlers: Arc::new(RwLock::new(HashMap::new())),
    };

    Router::new()
        .route("/", any(handle_dav::<F>))
        .route("/{*path}", any(handle_dav::<F>))
        .with_state(state)
        .layer(TraceLayer::new_for_http())
}

pub async fn handle_dav<F: DavFileSystem + Clone + Send + Sync + 'static>(
    State(state): State<AppState<F>>,
    auth: AuthBasic,
    req: Request,
) -> Response {
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

    let dav = {
        let handlers = state.handlers.read().unwrap();
        handlers.get(&user).cloned()
    };

    let dav = match dav {
        Some(handler) => handler,
        None => {
            let ufs = UserScopedFs::new(state.fs.clone(), user.clone());
            let _ = ufs.create_dir(&DavPath::new("/").unwrap()).await;

            let new_handler = DavHandler::builder()
                .filesystem(Box::new(ufs))
                .locksystem(FakeLs::new())
                .build_handler();

            let mut handlers = state.handlers.write().unwrap();
            handlers.entry(user).or_insert(new_handler).clone()
        }
    };

    let resp = dav.handle(req).await;
    let (parts, body) = resp.into_parts();
    Response::from_parts(parts, Body::new(body))
}
