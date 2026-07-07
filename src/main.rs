use std::collections::HashMap;

use mindav::app;
use mindav::config;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let conf = config::Config::load();

    let fs = mindav::minio::MinioFs::new(
        &conf.minio.endpoint,
        &conf.minio.bucket_name,
        conf.minio.ssl,
        &conf.minio.access_key,
        &conf.minio.secret_access_key,
        conf.app.upload_mode.clone(),
    )
    .await;

    let mut accounts: HashMap<String, String> = HashMap::new();
    for account in &conf.app.accounts {
        tracing::info!("Auth account: {}:******", account.username);
        accounts.insert(account.username.clone(), account.password.clone());
    }

    let app = app::build_router(fs, accounts);

    let addr = format!("0.0.0.0:{}", conf.app.port);
    tracing::info!("Starting server on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
