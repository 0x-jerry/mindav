use std::collections::HashMap;
use std::sync::Arc;
use std::time::SystemTime;

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ObjectIdentifier;
use aws_sdk_s3::Client;
use bytes::Bytes;
use dav_server::davpath::DavPath;
use dav_server::fs::{
    DavDirEntry, DavFile, DavFileSystem, DavMetaData, FsError, FsFuture, FsResult, FsStream,
    OpenOptions, ReadDirMeta,
};
use tokio::sync::RwLock;

use super::file::MinioFile;
use super::fileinfo::MinioMetaData;
use super::{clean_path_name, KEEP_FILE_CONTENT_TYPE, KEEP_FILE_NAME};

#[derive(Clone)]
pub struct MinioFs {
    client: Client,
    bucket: String,
    upload_mode: String,
    dirs: Arc<RwLock<HashMap<String, bool>>>,
}

fn datetime_to_systemtime(dt: &aws_smithy_types::DateTime) -> SystemTime {
    let secs = dt.as_secs_f64();
    std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(secs)
}

impl MinioFs {
    pub async fn new(
        endpoint: &str,
        bucket_name: &str,
        ssl: bool,
        access_key: &str,
        secret_access_key: &str,
        upload_mode: &str,
    ) -> Self {
        let scheme = if ssl { "https" } else { "http" };
        let endpoint_url = format!("{}://{}", scheme, endpoint);

        let credentials = Credentials::new(access_key, secret_access_key, None, None, "mindav");
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .endpoint_url(&endpoint_url)
            .credentials_provider(credentials)
            .load()
            .await;

        let s3_config = aws_sdk_s3::config::Builder::from(&config)
            .force_path_style(true)
            .build();
        let client = Client::from_conf(s3_config);

        tracing::info!("Login to {}", endpoint);

        MinioFs {
            client,
            bucket: bucket_name.to_string(),
            upload_mode: upload_mode.to_string(),
            dirs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    fn get_path(path: &DavPath) -> String {
        path.as_url_string()
    }

    async fn is_dir(&self, name: &str) -> bool {
        {
            let dirs = self.dirs.read().await;
            if let Some(&val) = dirs.get(name) {
                return val;
            }
        }

        let key = clean_path_name(name);
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(&key)
            .delimiter("/".to_string())
            .send()
            .await;

        let count = match resp {
            Ok(output) => {
                let common_prefixes = output.common_prefixes().len();
                let contents = output
                    .contents()
                    .iter()
                    .filter(|obj| {
                        obj.key()
                            .is_some_and(|k| k != key && k != format!("{}/", key))
                    })
                    .count();
                common_prefixes + contents
            }
            Err(_) => 0,
        };

        let is_dir = count > 0;
        {
            let mut dirs = self.dirs.write().await;
            dirs.insert(name.to_string(), is_dir);
        }
        is_dir
    }

    async fn reset_dir_check(&self, key_name: &str) {
        let mut dirs = self.dirs.write().await;
        dirs.retain(|k, _| !k.starts_with(key_name));
    }

    async fn list_objects_by_prefix(&self, prefix: &str) -> Vec<aws_sdk_s3::types::Object> {
        let key = clean_path_name(prefix);
        let mut objects = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut builder = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&key);

            if let Some(ref token) = continuation_token {
                builder = builder.continuation_token(token);
            }

            let resp = builder.send().await;

            match resp {
                Ok(output) => {
                    for obj in output.contents().iter() {
                        objects.push(obj.clone());
                    }
                    if output.is_truncated() == Some(true) {
                        continuation_token = output
                            .next_continuation_token()
                            .map(|s| s.to_string());
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!(
                        prefix = %key,
                        error = ?e,
                        "ListObjects failed"
                    );
                    break;
                }
            }
        }

        objects
    }

    async fn remove_all(&self, name: &str) -> FsResult<()> {
        let objects = self.list_objects_by_prefix(name).await;

        if !objects.is_empty() {
            let mut delete_objects: Vec<ObjectIdentifier> = Vec::new();
            for obj in &objects {
                if let Some(key) = obj.key() {
                    delete_objects.push(
                        ObjectIdentifier::builder()
                            .key(key.to_string())
                            .build()
                            .unwrap(),
                    );
                }
            }

            let result = self
                .client
                .delete_objects()
                .bucket(&self.bucket)
                .delete(
                    aws_sdk_s3::types::Delete::builder()
                        .set_objects(Some(delete_objects))
                        .build()
                        .unwrap(),
                )
                .send()
                .await;

            if let Err(e) = result {
                tracing::error!("RemoveAll failed: {}", e);
                return Err(FsError::GeneralFailure);
            }
        }

        let _ = self
            .client
            .delete_object()
            .bucket(&self.bucket)
            .key(name)
            .send()
            .await;

        self.reset_dir_check(name).await;

        Ok(())
    }
}

impl DavFileSystem for MinioFs {
    fn open<'a>(
        &'a self,
        path: &'a DavPath,
        options: OpenOptions,
    ) -> FsFuture<'a, Box<dyn DavFile>> {
        Box::pin(async move {
            let name = Self::get_path(path);
            let key = clean_path_name(&name);

            if options.write || options.create {
                let metadata = MinioMetaData::new_dir(format!("/{}", key));
                let file = MinioFile::new_write(
                    self.client.clone(),
                    self.bucket.clone(),
                    name,
                    self.upload_mode.clone(),
                    metadata,
                );
                Ok(Box::new(file) as Box<dyn DavFile>)
            } else {
                let result = self
                    .client
                    .head_object()
                    .bucket(&self.bucket)
                    .key(&key)
                    .send()
                    .await;

                let metadata = match result {
                    Ok(output) => MinioMetaData {
                        key: name.clone(),
                        size: output.content_length().unwrap_or(0) as u64,
                        last_modified: output
                            .last_modified()
                            .map(datetime_to_systemtime)
                            .unwrap_or_else(SystemTime::now),
                        is_dir: false,
                    },
                    Err(_) => MinioMetaData::new_dir(key.clone()),
                };

                let file = MinioFile::new_read(
                    self.client.clone(),
                    self.bucket.clone(),
                    name,
                    self.upload_mode.clone(),
                    metadata,
                )
                .await?;

                Ok(Box::new(file) as Box<dyn DavFile>)
            }
        })
    }

    fn read_dir<'a>(
        &'a self,
        path: &'a DavPath,
        _meta: ReadDirMeta,
    ) -> FsFuture<'a, FsStream<Box<dyn DavDirEntry>>> {
        Box::pin(async move {
            let name = Self::get_path(path);
            let mut prefix = clean_path_name(&name);

            if !prefix.ends_with('/') && prefix != "/" {
                prefix = format!("{}/", prefix);
            }

            if prefix == "/" {
                prefix = String::new();
            }

            let objects = self.list_objects_by_prefix(&prefix).await;
            let mut entries: Vec<Box<dyn DavDirEntry>> = Vec::new();

            for obj in objects {
                let k = obj.key().unwrap_or_default().to_string();
                if k.is_empty() || k == KEEP_FILE_NAME {
                    continue;
                }

                let is_dir_val = obj.storage_class().is_none()
                    && obj.e_tag().is_none()
                    && obj.size() == Some(0);

                let entry = MinioDirEntry {
                    key: k,
                    size: obj.size().unwrap_or(0) as u64,
                    last_modified: obj
                        .last_modified()
                        .map(datetime_to_systemtime)
                        .unwrap_or_else(SystemTime::now),
                    is_dir: is_dir_val,
                };

                entries.push(Box::new(entry));
            }

            let stream = futures_util::stream::iter(entries.into_iter().map(Ok));
            Ok(Box::pin(stream) as FsStream<Box<dyn DavDirEntry>>)
        })
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        Box::pin(async move {
            let name = Self::get_path(path);
            let key = clean_path_name(&name);

            if key == "/" || self.is_dir(&name).await {
                return Ok(Box::new(MinioMetaData::new_dir(name)) as Box<dyn DavMetaData>);
            }

            let result = self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(&key)
                .send()
                .await;

            match result {
                Ok(output) => {
                    let size = output.content_length().unwrap_or(0) as u64;
                    let last_modified = output
                        .last_modified()
                        .map(datetime_to_systemtime)
                        .unwrap_or_else(SystemTime::now);

                    Ok(Box::new(MinioMetaData {
                        key: name,
                        size,
                        last_modified,
                        is_dir: false,
                    }) as Box<dyn DavMetaData>)
                }
                Err(_) => Err(FsError::NotFound),
            }
        })
    }

    fn create_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let name = clean_path_name(&Self::get_path(path));
            let keep_path = if name == "/" {
                KEEP_FILE_NAME.to_string()
            } else {
                format!("{}/{}", name, KEEP_FILE_NAME)
            };

            let body = ByteStream::from(Bytes::new());
            let result = self
                .client
                .put_object()
                .bucket(&self.bucket)
                .key(&keep_path)
                .body(body)
                .content_type(KEEP_FILE_CONTENT_TYPE)
                .send()
                .await;

            match result {
                Ok(_) => {
                    tracing::info!("Mkdir success: {}", name);
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("Mkdir failed: {}", e);
                    Err(FsError::GeneralFailure)
                }
            }
        })
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let name = clean_path_name(&Self::get_path(path));
            self.remove_all(&name).await
        })
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let name = clean_path_name(&Self::get_path(path));
            let result = self
                .client
                .delete_object()
                .bucket(&self.bucket)
                .key(&name)
                .send()
                .await;

            match result {
                Ok(_) => {
                    tracing::info!("RemoveFile success: {}", name);
                    Ok(())
                }
                Err(e) => {
                    tracing::error!("RemoveFile failed: {}", e);
                    Err(FsError::GeneralFailure)
                }
            }
        })
    }

    fn rename<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let old_name = clean_path_name(&Self::get_path(from));
            let new_name = clean_path_name(&Self::get_path(to));

            tracing::info!("Rename: {} -> {}", old_name, new_name);

            let objects = self.list_objects_by_prefix(&old_name).await;

            for obj in &objects {
                let old_key = obj.key().unwrap_or_default();
                let new_key = old_key.replacen(&old_name, &new_name, 1);

                let result = self
                    .client
                    .copy_object()
                    .bucket(&self.bucket)
                    .copy_source(format!("{}/{}", &self.bucket, old_key))
                    .key(&new_key)
                    .send()
                    .await;

                match result {
                    Ok(_) => {
                        tracing::info!("Copy file success: {} -> {}", old_key, new_key);
                    }
                    Err(e) => {
                        tracing::error!("Copy file failed: {}", e);
                    }
                }
            }

            self.reset_dir_check(&old_name).await;
            let _ = self.remove_all(&old_name).await;

            tracing::info!("Rename success");
            Ok(())
        })
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let src_name = clean_path_name(&Self::get_path(from));
            let dst_name = clean_path_name(&Self::get_path(to));

            let objects = self.list_objects_by_prefix(&src_name).await;

            for obj in &objects {
                let old_key = obj.key().unwrap_or_default();
                let new_key = old_key.replacen(&src_name, &dst_name, 1);

                let result = self
                    .client
                    .copy_object()
                    .bucket(&self.bucket)
                    .copy_source(format!("{}/{}", &self.bucket, old_key))
                    .key(&new_key)
                    .send()
                    .await;

                match result {
                    Ok(_) => {
                        tracing::info!("Copy file success: {} -> {}", old_key, new_key);
                    }
                    Err(e) => {
                        tracing::error!("Copy file failed: {}", e);
                    }
                }
            }

            tracing::info!("Copy success");
            Ok(())
        })
    }
}

struct MinioDirEntry {
    key: String,
    size: u64,
    last_modified: SystemTime,
    is_dir: bool,
}

impl DavDirEntry for MinioDirEntry {
    fn name(&self) -> Vec<u8> {
        let name = if self.is_dir {
            self.key.trim_matches('/').to_string()
        } else {
            self.key.clone()
        };

        if let Some(pos) = name.rfind('/') {
            name.as_bytes()[(pos + 1)..].to_vec()
        } else {
            name.as_bytes().to_vec()
        }
    }

    fn metadata(&self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let metadata = MinioMetaData {
            key: self.key.clone(),
            size: self.size,
            last_modified: self.last_modified,
            is_dir: self.is_dir,
        };
        Box::pin(async move { Ok(Box::new(metadata) as Box<dyn DavMetaData>) })
    }
}
