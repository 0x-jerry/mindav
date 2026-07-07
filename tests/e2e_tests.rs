use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use mindav::app;
use mindav::minio::{MinioFs, MinioFsConfig, UploadMode};
use reqwest_dav::{Auth, ClientBuilder, Depth};
use reqwest_dav::types::list_cmd::ListEntity;
use tokio::sync::OnceCell;

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};

static COUNTER: AtomicUsize = AtomicUsize::new(0);
static BUCKET_READY: OnceCell<()> = OnceCell::const_new();

const S3_ENDPOINT: &str = "http://localhost:9000";
const BUCKET: &str = "test";
const ACCESS_KEY: &str = "rustfsadmin";
const SECRET_KEY: &str = "rustfsadmin";
const TEST_PASSWORD: &str = "testpass";

fn next_username() -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("e2e-user-{}", n)
}

async fn s3_client() -> aws_sdk_s3::Client {
    let credentials = Credentials::new(ACCESS_KEY, SECRET_KEY, None, None, "test");
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(S3_ENDPOINT)
        .credentials_provider(credentials)
        .load()
        .await;
    let s3_config = aws_sdk_s3::config::Builder::from(&config)
        .force_path_style(true)
        .build();
    aws_sdk_s3::Client::from_conf(s3_config)
}

async fn ensure_bucket() {
    let client = s3_client().await;
    if client
        .list_buckets()
        .send()
        .await
        .map(|r| r.buckets().iter().any(|b| b.name() == Some(BUCKET)))
        .unwrap_or(false)
    {
        return;
    }
    client
        .create_bucket()
        .bucket(BUCKET)
        .send()
        .await
        .expect("create bucket");
}

async fn init_bucket() {
    BUCKET_READY
        .get_or_init(|| async { ensure_bucket().await })
        .await;
}

async fn create_test_fs(upload_mode: UploadMode) -> MinioFs {
    MinioFs::new(&MinioFsConfig {
        endpoint: "localhost:9000".into(),
        bucket_name: BUCKET.into(),
        ssl: false,
        access_key: ACCESS_KEY.into(),
        secret_access_key: SECRET_KEY.into(),
        upload_mode,
    })
    .await
}

async fn spawn_server(
    username: &str,
    upload_mode: UploadMode,
) -> (reqwest_dav::Client, MinioFs, tokio::task::JoinHandle<()>) {
    init_bucket().await;
    let fs = create_test_fs(upload_mode).await;
    let accounts = HashMap::from([(username.to_string(), TEST_PASSWORD.to_string())]);
    let router = app::build_router(fs.clone(), accounts);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let host = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let client = ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Basic(username.into(), TEST_PASSWORD.into()))
        .build()
        .expect("build client");

    (client, fs, handle)
}

async fn cleanup_user(fs: &MinioFs, username: &str) {
    use dav_server::davpath::DavPath;
    use dav_server::fs::DavFileSystem;
    let path = DavPath::new(&format!("/{}", username)).unwrap();
    let _ = fs.remove_dir(&path).await;
}

#[tokio::test]
async fn auth_invalid_password() {
    let username = next_username();
    init_bucket().await;
    let fs = create_test_fs(UploadMode::File).await;
    let accounts = HashMap::from([(username.clone(), TEST_PASSWORD.to_string())]);
    let router = app::build_router(fs, accounts);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let host = format!("http://{}", addr);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let client = ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Basic(username, "wrongpass".into()))
        .build()
        .expect("build client");

    let result = client.list("/", Depth::Number(1)).await;
    assert!(result.is_err(), "invalid password should fail");
}

#[tokio::test]
async fn auth_missing_credentials() {
    let username = next_username();
    init_bucket().await;
    let fs = create_test_fs(UploadMode::File).await;
    let accounts = HashMap::from([(username, TEST_PASSWORD.to_string())]);
    let router = app::build_router(fs, accounts);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let host = format!("http://{}", addr);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let client = ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Anonymous)
        .build()
        .expect("build client");

    let result = client.list("/", Depth::Number(1)).await;
    assert!(result.is_err(), "missing credentials should fail");
}

#[tokio::test]
async fn new_user_no_404() {
    let username = next_username();
    let (client, fs, _handle) = spawn_server(&username, UploadMode::File).await;

    let entries = client.list("/", Depth::Number(1)).await.expect("list root");
    assert!(!entries.is_empty(), "new user root should have at least self directory");

    cleanup_user(&fs, &username).await;
}

#[tokio::test]
async fn mkcol_and_list_directory() {
    let username = next_username();
    let (client, fs, _handle) = spawn_server(&username, UploadMode::File).await;

    client.mkcol("/docs").await.expect("mkcol");

    let entries = client.list("/", Depth::Number(1)).await.expect("list");
    let has_docs = entries.iter().any(|e| match e {
        ListEntity::Folder(f) => f.href.ends_with("/docs/"),
        _ => false,
    });
    assert!(has_docs, "root listing should contain docs dir after mkcol");

    cleanup_user(&fs, &username).await;
}

#[tokio::test]
async fn put_and_get_file() {
    let username = next_username();
    let (client, fs, _handle) = spawn_server(&username, UploadMode::File).await;

    let content = b"Hello, WebDAV E2E test!";
    client.put("/hello.txt", content.as_slice()).await.expect("put");

    let resp = client.get_raw("/hello.txt").await.expect("get_raw");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(&body[..], content, "GET body should match PUT content");

    cleanup_user(&fs, &username).await;
}

#[tokio::test]
async fn delete_file() {
    let username = next_username();
    let (client, fs, _handle) = spawn_server(&username, UploadMode::File).await;

    client
        .put("/tmp.txt", b"delete me".as_slice())
        .await
        .expect("put");
    client.delete("/tmp.txt").await.expect("delete");

    let result = client.get_raw("/tmp.txt").await;
    assert!(
        result.is_err() || result.unwrap().status().as_u16() >= 400,
        "GET after delete should fail"
    );

    cleanup_user(&fs, &username).await;
}

#[tokio::test]
async fn move_file() {
    let username = next_username();
    let (client, fs, _handle) = spawn_server(&username, UploadMode::File).await;

    let content = b"move me please";
    client.put("/a.txt", content.as_slice()).await.expect("put");

    client.mv("/a.txt", "/b.txt").await.expect("move");

    let resp = client.get_raw("/b.txt").await.expect("get b.txt");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(&body[..], content, "moved file should have original content");

    let result = client.get_raw("/a.txt").await;
    assert!(
        result.is_err() || result.unwrap().status().as_u16() >= 400,
        "original path should not exist after move"
    );

    cleanup_user(&fs, &username).await;
}

#[tokio::test]
async fn copy_file() {
    let username = next_username();
    let (client, fs, _handle) = spawn_server(&username, UploadMode::File).await;

    let content = b"copy me please";
    client.put("/orig.txt", content.as_slice()).await.expect("put");

    client.cp("/orig.txt", "/dup.txt").await.expect("copy");

    let resp = client.get_raw("/orig.txt").await.expect("get orig");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(&body[..], content, "original should remain unchanged");

    let resp = client.get_raw("/dup.txt").await.expect("get dup");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(&body[..], content, "copy should have same content");

    cleanup_user(&fs, &username).await;
}

#[tokio::test]
async fn namespace_isolation() {
    let alice_name = next_username();
    let bob_name = next_username();
    init_bucket().await;

    let fs = create_test_fs(UploadMode::File).await;
    let accounts = HashMap::from([
        (alice_name.clone(), TEST_PASSWORD.to_string()),
        (bob_name.clone(), TEST_PASSWORD.to_string()),
    ]);
    let router = app::build_router(fs.clone(), accounts);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("local_addr");
    let host = format!("http://{}", addr);
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let alice = ClientBuilder::new()
        .set_host(host.clone())
        .set_auth(Auth::Basic(alice_name.clone(), TEST_PASSWORD.into()))
        .build()
        .expect("build alice");
    let bob = ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Basic(bob_name.clone(), TEST_PASSWORD.into()))
        .build()
        .expect("build bob");

    alice
        .put("/secret.txt", b"alice's secret".as_slice())
        .await
        .expect("put");

    let bob_entries = bob.list("/", Depth::Number(1)).await.expect("bob list");

    let alice_file_visible = bob_entries.iter().any(|e| match e {
        ListEntity::File(f) => f.href.contains("secret.txt"),
        _ => false,
    });
    assert!(!alice_file_visible, "bob should not see alice's files");

    cleanup_user(&fs, &alice_name).await;
    cleanup_user(&fs, &bob_name).await;
}

#[tokio::test]
async fn upload_mode_memory_e2e() {
    let username = next_username();
    let (client, fs, _handle) = spawn_server(&username, UploadMode::Memory).await;

    let content = b"uploaded in memory mode via WebDAV";
    client.put("/mem.txt", content.as_slice()).await.expect("put");

    let resp = client.get_raw("/mem.txt").await.expect("get_raw");
    let body = resp.bytes().await.expect("read body");
    assert_eq!(&body[..], content, "memory upload mode: GET body should match PUT content");

    let entries = client.list("/", Depth::Number(1)).await.expect("list");
    let has_mem = entries.iter().any(|e| match e {
        ListEntity::File(f) => f.href.ends_with("/mem.txt") || f.href.ends_with("mem.txt"),
        _ => false,
    });
    assert!(has_mem, "memory mode: listing should contain uploaded file");

    client.delete("/mem.txt").await.expect("delete");

    cleanup_user(&fs, &username).await;
}
