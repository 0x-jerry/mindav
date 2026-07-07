use std::sync::atomic::{AtomicUsize, Ordering};

use aws_config::BehaviorVersion;
use aws_sdk_s3::config::{Credentials, Region};
use dav_server::davpath::DavPath;
use dav_server::fs::{DavFileSystem, FsError, OpenOptions, ReadDirMeta};
use futures_util::StreamExt;
use mindav::minio::{MinioFs, UploadMode};
use tokio::sync::OnceCell;

static COUNTER: AtomicUsize = AtomicUsize::new(0);
static BUCKET_READY: OnceCell<()> = OnceCell::const_new();

const S3_ENDPOINT: &str = "http://localhost:9000";
const BUCKET: &str = "test";
const ACCESS_KEY: &str = "rustfsadmin";
const SECRET_KEY: &str = "rustfsadmin";

fn next_prefix(name: &str) -> String {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("test-{}-{}", name, n)
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
        .get_or_init(|| async {
            ensure_bucket().await;
        })
        .await;
}

async fn create_test_fs(upload_mode: UploadMode) -> MinioFs {
    MinioFs::new(
        "localhost:9000",
        BUCKET,
        false,
        ACCESS_KEY,
        SECRET_KEY,
        upload_mode,
    )
    .await
}

async fn cleanup(fs: &MinioFs, prefix: &str) {
    let path = DavPath::new(&format!("/{}", prefix.trim_end_matches('/'))).unwrap();
    let _ = fs.remove_dir(&path).await;
}

fn read_only() -> OpenOptions {
    OpenOptions {
        read: true,
        ..Default::default()
    }
}

fn write_create() -> OpenOptions {
    OpenOptions {
        write: true,
        create: true,
        truncate: true,
        ..Default::default()
    }
}

fn dav_path(s: &str) -> DavPath {
    DavPath::new(s).unwrap()
}

#[tokio::test]
async fn create_and_list_dirs() {
    init_bucket().await;
    let prefix = next_prefix("dirs");
    let fs = create_test_fs(UploadMode::File).await;

    let root = dav_path(&format!("/{}", prefix));
    let sub = dav_path(&format!("/{}/sub", prefix));

    fs.create_dir(&root).await.expect("create root dir");
    fs.create_dir(&sub).await.expect("create sub dir");

    let mut stream = fs
        .read_dir(&root, ReadDirMeta::None)
        .await
        .expect("read_dir root");
    let mut names: Vec<String> = vec![];
    while let Some(entry) = stream.next().await {
        let entry = entry.expect("entry");
        names.push(String::from_utf8_lossy(&entry.name()).to_string());
    }
    assert!(names.contains(&"sub".to_string()), "root should contain sub dir: {names:?}");

    let stream = fs
        .read_dir(&sub, ReadDirMeta::None)
        .await
        .expect("read_dir sub");
    let entries: Vec<_> = stream.collect::<Vec<_>>().await;
    assert!(
        entries.iter().all(|e| e.is_ok()),
        "empty dir listing should have no errors"
    );

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn write_and_read_file() {
    init_bucket().await;
    let prefix = next_prefix("wr");
    let fs = create_test_fs(UploadMode::File).await;

    let path = dav_path(&format!("/{}/hello.txt", prefix));
    let content = b"Hello, WebDAV world!";

    let mut file = fs.open(&path, write_create()).await.expect("open for write");
    file.write_bytes(bytes::Bytes::from_static(content))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    let mut file = fs.open(&path, read_only()).await.expect("open for read");
    let data = file.read_bytes(1024).await.expect("read");
    assert_eq!(&data[..], content);

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn write_empty_file() {
    init_bucket().await;
    let prefix = next_prefix("empty");
    let fs = create_test_fs(UploadMode::File).await;

    let path = dav_path(&format!("/{}/empty.txt", prefix));

    let mut file = fs.open(&path, write_create()).await.expect("open");
    file.flush().await.expect("flush");

    let mut file = fs.open(&path, read_only()).await.expect("open read");
    let data = file.read_bytes(1024).await.expect("read");
    assert!(data.is_empty(), "empty file should return empty bytes");

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn write_large_and_seek() {
    init_bucket().await;
    let prefix = next_prefix("large");
    let fs = create_test_fs(UploadMode::File).await;

    let path = dav_path(&format!("/{}/large.bin", prefix));

    let megabyte: Vec<u8> = (0..(1024 * 1024))
        .map(|i| (i % 256) as u8)
        .collect();

    let mut file = fs.open(&path, write_create()).await.expect("open write");
    file.write_bytes(bytes::Bytes::from(megabyte.clone()))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    let mut file = fs.open(&path, read_only()).await.expect("open read");

    let offset = 500_000u64;
    let count = 200usize;
    file.seek(std::io::SeekFrom::Start(offset))
        .await
        .expect("seek");
    let chunk = file.read_bytes(count).await.expect("read chunk");
    assert_eq!(chunk.len(), count);
    for (i, &byte) in chunk.iter().enumerate() {
        let expected = ((offset as usize + i) % 256) as u8;
        assert_eq!(byte, expected, "mismatch at offset {}", offset as usize + i);
    }

    let end_pos = file
        .seek(std::io::SeekFrom::End(-100))
        .await
        .expect("seek end");
    let tail = file.read_bytes(50).await.expect("read tail");
    assert_eq!(tail.len(), 50);
    let expected_end = (megabyte.len() - 100) as u64;
    assert!(end_pos >= expected_end, "seek end should be near file end");

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn file_metadata() {
    init_bucket().await;
    let prefix = next_prefix("meta");
    let fs = create_test_fs(UploadMode::File).await;

    let path = dav_path(&format!("/{}/data.txt", prefix));
    let content = vec![0x41u8; 1234];

    let mut file = fs.open(&path, write_create()).await.expect("open write");
    file.write_bytes(bytes::Bytes::from(content))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    let meta = fs.metadata(&path).await.expect("metadata");
    assert_eq!(meta.len(), 1234);
    assert!(!meta.is_dir());

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn dir_metadata() {
    init_bucket().await;
    let prefix = next_prefix("dirmeta");
    let fs = create_test_fs(UploadMode::File).await;

    let dir = dav_path(&format!("/{}", prefix));
    fs.create_dir(&dir).await.expect("create dir");

    let meta = fs.metadata(&dir).await.expect("dir metadata");
    assert!(meta.is_dir());
    assert_eq!(meta.len(), 0);

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn remove_file() {
    init_bucket().await;
    let prefix = next_prefix("rmfile");
    let fs = create_test_fs(UploadMode::File).await;

    let path = dav_path(&format!("/{}/rm.txt", prefix));

    let mut file = fs.open(&path, write_create()).await.expect("open write");
    file.write_bytes(bytes::Bytes::from_static(b"delete me"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    fs.remove_file(&path).await.expect("remove file");

    let err = fs.open(&path, read_only()).await;
    assert!(err.is_err(), "open after remove should fail");

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn remove_dir_recursive() {
    init_bucket().await;
    let prefix = next_prefix("rmdir");
    let fs = create_test_fs(UploadMode::File).await;

    let dir = dav_path(&format!("/{}", prefix));
    fs.create_dir(&dir).await.expect("create dir");

    let f1 = dav_path(&format!("/{}/a.txt", prefix));
    let mut file = fs.open(&f1, write_create()).await.expect("open a.txt");
    file.write_bytes(bytes::Bytes::from_static(b"file a"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    let sub = dav_path(&format!("/{}/sub", prefix));
    fs.create_dir(&sub).await.expect("create sub");

    let f2 = dav_path(&format!("/{}/sub/b.txt", prefix));
    let mut file = fs.open(&f2, write_create()).await.expect("open b.txt");
    file.write_bytes(bytes::Bytes::from_static(b"file b"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    fs.remove_dir(&dir).await.expect("remove dir recursive");

    let err = fs.open(&f1, read_only()).await;
    assert!(err.is_err(), "top-level file should be gone after remove");

    let err = fs.open(&f2, read_only()).await;
    assert!(err.is_err(), "nested file should be gone after remove");

    let stream = fs
        .read_dir(&dir, ReadDirMeta::None)
        .await
        .expect("read_dir should not fail");
    let remaining: Vec<_> = stream.collect::<Vec<_>>().await;
    assert!(
        remaining.iter().all(|e| e.is_ok()),
        "removed dir should return empty listing"
    );

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn rename_file() {
    init_bucket().await;
    let src_prefix = next_prefix("rnsrc");
    let dst_prefix = next_prefix("rndst");
    let fs = create_test_fs(UploadMode::File).await;

    let src = dav_path(&format!("/{}/file.txt", src_prefix));
    let dst = dav_path(&format!("/{}/moved.txt", dst_prefix));

    let dir = dav_path(&format!("/{}", src_prefix));
    fs.create_dir(&dir).await.expect("create src dir");
    let dir = dav_path(&format!("/{}", dst_prefix));
    fs.create_dir(&dir).await.expect("create dst dir");

    let mut file = fs.open(&src, write_create()).await.expect("open write");
    file.write_bytes(bytes::Bytes::from_static(b"rename me"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    fs.rename(&src, &dst).await.expect("rename");

    let mut file = fs.open(&dst, read_only()).await.expect("open dst after rename");
    let data = file.read_bytes(1024).await.expect("read dst");
    assert_eq!(&data[..], b"rename me");

    let err = fs.open(&src, read_only()).await;
    assert!(err.is_err(), "src should not exist after rename");

    cleanup(&fs, &src_prefix).await;
    cleanup(&fs, &dst_prefix).await;
}

#[tokio::test]
async fn rename_directory() {
    init_bucket().await;
    let src_prefix = next_prefix("rndirsrc");
    let dst_prefix = next_prefix("rndirdst");
    let fs = create_test_fs(UploadMode::File).await;

    let src_dir = dav_path(&format!("/{}", src_prefix));
    let dst_dir = dav_path(&format!("/{}", dst_prefix));

    fs.create_dir(&src_dir).await.expect("create src dir");

    let f = dav_path(&format!("/{}/a.txt", src_prefix));
    let mut file = fs.open(&f, write_create()).await.expect("open a.txt");
    file.write_bytes(bytes::Bytes::from_static(b"alpha"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    let sub = dav_path(&format!("/{}/sub", src_prefix));
    fs.create_dir(&sub).await.expect("create sub");

    let f2 = dav_path(&format!("/{}/sub/b.txt", src_prefix));
    let mut file = fs.open(&f2, write_create()).await.expect("open b.txt");
    file.write_bytes(bytes::Bytes::from_static(b"beta"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    fs.rename(&src_dir, &dst_dir).await.expect("rename dir");

    let mut stream = fs
        .read_dir(&dst_dir, ReadDirMeta::None)
        .await
        .expect("read_dir dst");
    let mut names: Vec<String> = vec![];
    while let Some(entry) = stream.next().await {
        let entry = entry.expect("entry");
        names.push(String::from_utf8_lossy(&entry.name()).to_string());
    }
    assert!(names.contains(&"a.txt".to_string()), "dst should contain a.txt: {names:?}");
    assert!(names.contains(&"sub".to_string()), "dst should contain sub: {names:?}");

    let f_moved = dav_path(&format!("/{}/a.txt", dst_prefix));
    let mut file = fs.open(&f_moved, read_only()).await.expect("open moved a.txt");
    let data = file.read_bytes(1024).await.expect("read");
    assert_eq!(&data[..], b"alpha");

    let f_moved = dav_path(&format!("/{}/sub/b.txt", dst_prefix));
    let mut file = fs.open(&f_moved, read_only()).await.expect("open moved b.txt");
    let data = file.read_bytes(1024).await.expect("read");
    assert_eq!(&data[..], b"beta");

    let err = fs.open(&f, read_only()).await;
    assert!(err.is_err(), "src file should not exist after rename");

    cleanup(&fs, &src_prefix).await;
    cleanup(&fs, &dst_prefix).await;
}

#[tokio::test]
async fn copy_file() {
    init_bucket().await;
    let src_prefix = next_prefix("cpsrc");
    let dst_prefix = next_prefix("cpdst");
    let fs = create_test_fs(UploadMode::File).await;

    let src = dav_path(&format!("/{}/original.txt", src_prefix));
    let dst = dav_path(&format!("/{}/duplicate.txt", dst_prefix));

    let dir = dav_path(&format!("/{}", src_prefix));
    fs.create_dir(&dir).await.expect("create src dir");
    let dir = dav_path(&format!("/{}", dst_prefix));
    fs.create_dir(&dir).await.expect("create dst dir");

    let mut file = fs.open(&src, write_create()).await.expect("open write");
    file.write_bytes(bytes::Bytes::from_static(b"copy me"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    fs.copy(&src, &dst).await.expect("copy");

    let mut file = fs.open(&src, read_only()).await.expect("open src after copy");
    let data = file.read_bytes(1024).await.expect("read src");
    assert_eq!(&data[..], b"copy me", "original should remain");

    let mut file = fs.open(&dst, read_only()).await.expect("open dst after copy");
    let data = file.read_bytes(1024).await.expect("read dst");
    assert_eq!(&data[..], b"copy me", "copy should have same content");

    cleanup(&fs, &src_prefix).await;
    cleanup(&fs, &dst_prefix).await;
}

#[tokio::test]
async fn copy_directory() {
    init_bucket().await;
    let src_prefix = next_prefix("cpdirsrc");
    let dst_prefix = next_prefix("cpdirdst");
    let fs = create_test_fs(UploadMode::File).await;

    let src_dir = dav_path(&format!("/{}", src_prefix));
    let dst_dir = dav_path(&format!("/{}", dst_prefix));

    fs.create_dir(&src_dir).await.expect("create src dir");

    let f1 = dav_path(&format!("/{}/x.txt", src_prefix));
    let mut file = fs.open(&f1, write_create()).await.expect("open x.txt");
    file.write_bytes(bytes::Bytes::from_static(b"x-ray"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    let sub = dav_path(&format!("/{}/nested", src_prefix));
    fs.create_dir(&sub).await.expect("create nested");

    let f2 = dav_path(&format!("/{}/nested/y.txt", src_prefix));
    let mut file = fs.open(&f2, write_create()).await.expect("open y.txt");
    file.write_bytes(bytes::Bytes::from_static(b"yankee"))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    fs.copy(&src_dir, &dst_dir).await.expect("copy dir");

    let mut stream = fs
        .read_dir(&dst_dir, ReadDirMeta::None)
        .await
        .expect("read_dir dst");
    let mut names: Vec<String> = vec![];
    while let Some(entry) = stream.next().await {
        let entry = entry.expect("entry");
        names.push(String::from_utf8_lossy(&entry.name()).to_string());
    }
    assert!(
        names.contains(&"x.txt".to_string()),
        "dst should contain x.txt: {names:?}"
    );
    assert!(
        names.contains(&"nested".to_string()),
        "dst should contain nested: {names:?}"
    );

    let f_copied = dav_path(&format!("/{}/x.txt", dst_prefix));
    let mut file = fs.open(&f_copied, read_only()).await.expect("open copied x.txt");
    let data = file.read_bytes(1024).await.expect("read");
    assert_eq!(&data[..], b"x-ray");

    let f_copied = dav_path(&format!("/{}/nested/y.txt", dst_prefix));
    let mut file = fs.open(&f_copied, read_only()).await.expect("open copied y.txt");
    let data = file.read_bytes(1024).await.expect("read");
    assert_eq!(&data[..], b"yankee");

    let f_orig = dav_path(&format!("/{}/x.txt", src_prefix));
    let mut file = fs.open(&f_orig, read_only()).await.expect("open original x.txt");
    let data = file.read_bytes(1024).await.expect("read");
    assert_eq!(&data[..], b"x-ray", "original should remain unchanged");

    cleanup(&fs, &src_prefix).await;
    cleanup(&fs, &dst_prefix).await;
}

#[tokio::test]
async fn not_found_errors() {
    init_bucket().await;
    let prefix = next_prefix("notfound");
    let fs = create_test_fs(UploadMode::File).await;

    let nonexistent = dav_path(&format!("/{}/does_not_exist.txt", prefix));

    let err = fs.open(&nonexistent, read_only()).await;
    assert!(matches!(err, Err(FsError::NotFound)), "open should return NotFound");

    let err = fs.metadata(&nonexistent).await;
    assert!(
        matches!(err, Err(FsError::NotFound)),
        "metadata should return NotFound"
    );

    let mut stream = fs
        .read_dir(&nonexistent, ReadDirMeta::None)
        .await
        .expect("read_dir should not fail immediately");
    let first = stream.next().await;
    assert!(first.is_none(), "non-existent dir should return empty stream");

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn create_dir_keep_file() {
    init_bucket().await;
    let prefix = next_prefix("keep");
    let fs = create_test_fs(UploadMode::File).await;

    let dir = dav_path(&format!("/{}", prefix));
    fs.create_dir(&dir).await.expect("create dir");

    let client = s3_client().await;
    let key = format!("{}/.mindavkeep", prefix);
    let resp = client.head_object().bucket(BUCKET).key(&key).send().await;
    assert!(resp.is_ok(), ".mindavkeep file should exist in S3");

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn upload_mode_memory() {
    init_bucket().await;
    let prefix = next_prefix("memory");
    let fs = create_test_fs(UploadMode::Memory).await;

    let path = dav_path(&format!("/{}/mem.txt", prefix));
    let content = b"uploaded in memory mode";

    let mut file = fs.open(&path, write_create()).await.expect("open write");
    file.write_bytes(bytes::Bytes::from_static(content))
        .await
        .expect("write");
    file.flush().await.expect("flush");

    let mut file = fs.open(&path, read_only()).await.expect("open read");
    let data = file.read_bytes(1024).await.expect("read");
    assert_eq!(&data[..], content);

    let meta = fs.metadata(&path).await.expect("metadata");
    assert_eq!(meta.len(), content.len() as u64);

    cleanup(&fs, &prefix).await;
}

#[tokio::test]
async fn root_listing_includes_test_dirs() {
    init_bucket().await;
    let prefix = next_prefix("rootlist");
    let fs = create_test_fs(UploadMode::File).await;

    let dir = dav_path(&format!("/{}", prefix));
    fs.create_dir(&dir).await.expect("create dir");

    let root = dav_path("/");
    let mut stream = fs
        .read_dir(&root, ReadDirMeta::None)
        .await
        .expect("read_dir /");
    let mut found = false;
    while let Some(entry) = stream.next().await {
        let entry = entry.expect("entry");
        if String::from_utf8_lossy(&entry.name()) == prefix {
            found = true;
            break;
        }
    }
    assert!(found, "root listing should contain test prefix dir: {prefix}");

    cleanup(&fs, &prefix).await;
}
