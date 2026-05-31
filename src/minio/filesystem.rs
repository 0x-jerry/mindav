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

use crate::minio::dir::MinioDirEntry;

use super::file::MinioFile;
use super::fileinfo::MinioMetaData;
use super::{KEEP_FILE_CONTENT_TYPE, KEEP_FILE_NAME};

#[derive(Clone)]
pub struct MinioFs {
    client: Client,
    bucket: String,
    upload_mode: String,
}

fn datetime_to_systemtime(dt: &aws_smithy_types::DateTime) -> SystemTime {
    let secs = dt.as_secs_f64();
    std::time::UNIX_EPOCH + std::time::Duration::from_secs_f64(secs)
}

fn url_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for &byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    result
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
        }
    }

    fn get_path(path: &DavPath) -> String {
        let path = path.to_string().trim_start_matches("/").to_string();
        path
    }

    async fn is_dir(&self, name: &str) -> bool {
        let result = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(name)
            .delimiter("/")
            .send()
            .await;

        if let Ok(r) = result {
            if r.key_count.unwrap_or_default() > 0 {
                return true;
            }
        }

        return false;
    }

    async fn list_objects_by_prefix(&self, prefix: &str) -> Vec<aws_sdk_s3::types::Object> {
        let key = prefix;
        let mut objects = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut builder = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(key)
                .delimiter("/");

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
                        continuation_token =
                            output.next_continuation_token().map(|s| s.to_string());
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

    async fn list_dir_entries(&self, prefix: &str) -> Vec<Box<dyn DavDirEntry>> {
        let mut key = prefix.to_string();
        if !prefix.is_empty() {
            key = format!("{}/", prefix);
        }

        let mut entries: Vec<Box<dyn DavDirEntry>> = Vec::new();
        let mut continuation_token: Option<String> = None;

        loop {
            let mut builder = self
                .client
                .list_objects_v2()
                .bucket(&self.bucket)
                .prefix(&key)
                .delimiter("/".to_string());

            if let Some(ref token) = continuation_token {
                builder = builder.continuation_token(token);
            }

            let resp = builder.send().await;

            match resp {
                Ok(output) => {
                    for cp in output.common_prefixes().iter() {
                        let Some(key) = cp.prefix() else { continue };
                        let entry = MinioDirEntry {
                            key: key.to_string(),
                            size: 0,
                            last_modified: SystemTime::now(),
                            is_dir: true,
                        };
                        entries.push(Box::new(entry));
                    }

                    for obj in output.contents().iter() {
                        let k = obj.key().unwrap_or_default().to_string();
                        if k == KEEP_FILE_NAME || k.ends_with(&format!("/{}", KEEP_FILE_NAME)) {
                            continue;
                        }
                        if k == key || k == format!("{}/", key) {
                            continue;
                        }
                        let entry = MinioDirEntry {
                            key: k,
                            size: obj.size().unwrap_or(0) as u64,
                            last_modified: obj
                                .last_modified()
                                .map(datetime_to_systemtime)
                                .unwrap_or_else(SystemTime::now),
                            is_dir: false,
                        };
                        entries.push(Box::new(entry));
                    }

                    if output.is_truncated() == Some(true) {
                        continuation_token =
                            output.next_continuation_token().map(|s| s.to_string());
                    } else {
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!(
                        prefix = %key,
                        error = ?e,
                        "ListDirEntries failed"
                    );
                    break;
                }
            }
        }

        entries
    }

    async fn remove_all(&self, name: &str) -> FsResult<()> {
        if name.is_empty() {
            tracing::error!("RemoveAll: not allowed to delete root");
            return Err(FsError::GeneralFailure);
        }

        let prefix = format!("{}/", name);

        tracing::info!("delete dir: {}", prefix);

        let objects = self.list_objects_by_prefix(&prefix).await;

        if !objects.is_empty() {
            let delete_objects: Vec<ObjectIdentifier> = objects
                .iter()
                .filter_map(|obj| obj.key().map(|k| k.to_string()))
                .map(|key| ObjectIdentifier::builder().key(key).build().unwrap())
                .collect();

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

            tracing::info!("open file {}, opt {:?}", name, options);

            if options.write || options.create {
                let metadata = MinioMetaData::new_dir(format!("{}", name));
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
                    .key(&name)
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
                    Err(_) => return Err(FsError::NotFound),
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

            tracing::info!("open dir: {}", name);

            let entries = self.list_dir_entries(&name).await;
            let stream = futures_util::stream::iter(entries.into_iter().map(Ok));
            Ok(Box::pin(stream) as FsStream<Box<dyn DavDirEntry>>)
        })
    }

    fn metadata<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, Box<dyn DavMetaData>> {
        Box::pin(async move {
            let name = Self::get_path(path);

            tracing::info!("metadata: {}", name);

            if name.is_empty() || self.is_dir(&name).await {
                return Ok(Box::new(MinioMetaData::new_dir(name)) as Box<dyn DavMetaData>);
            }

            let result = self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(&name)
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
            let name = Self::get_path(path);
            tracing::info!("create dir: {}", name);

            let keep_path = if name.is_empty() {
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
                    tracing::error!("Mkdir failed on {}: {:?}", keep_path, e);
                    Err(FsError::GeneralFailure)
                }
            }
        })
    }

    fn remove_dir<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let name = Self::get_path(path);
            tracing::info!("remove dir: {}", name);
            self.remove_all(&name).await
        })
    }

    fn remove_file<'a>(&'a self, path: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let name = Self::get_path(path);
            tracing::info!("remove file: {}", name);
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
            let old_name = Self::get_path(from);
            let new_name = Self::get_path(to);

            tracing::info!("Rename: {} -> {}", old_name, new_name);

            let is_file = self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(&old_name)
                .send()
                .await
                .is_ok();

            if is_file {
                let copy_source = url_encode(&format!("{}/{}", &self.bucket, &old_name));
                self.client
                    .copy_object()
                    .bucket(&self.bucket)
                    .copy_source(copy_source)
                    .key(&new_name)
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::error!("Rename copy failed: {}", e);
                        FsError::GeneralFailure
                    })?;

                self.client
                    .delete_object()
                    .bucket(&self.bucket)
                    .key(&old_name)
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::error!("Rename delete failed: {}", e);
                        FsError::GeneralFailure
                    })?;

                return Ok(());
            }

            let is_dir = self.is_dir(&old_name).await;
            if !is_dir || old_name.is_empty() {
                return Err(FsError::NotFound);
            }

            let prefix = format!("{}/", old_name);

            let objects = self.list_objects_by_prefix(&prefix).await;

            for obj in &objects {
                let old_key = obj.key().unwrap_or_default();
                let new_key = old_key.replacen(&old_name, &new_name, 1);

                tracing::info!("COPY {} -> {}", old_key, new_key);

                let copy_source = url_encode(&format!("{}/{}", &self.bucket, &old_key));
                self.client
                    .copy_object()
                    .bucket(&self.bucket)
                    .copy_source(copy_source)
                    .key(&new_key)
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::error!("Rename copy failed for {}: {:?}", old_key, e);
                        FsError::GeneralFailure
                    })?;
            }

            self.remove_all(&old_name).await?;

            tracing::info!("Rename success");
            Ok(())
        })
    }

    fn copy<'a>(&'a self, from: &'a DavPath, to: &'a DavPath) -> FsFuture<'a, ()> {
        Box::pin(async move {
            let src_name = Self::get_path(from);
            let dst_name = Self::get_path(to);

            tracing::info!("Copy: {} -> {}", src_name, dst_name);

            let is_file = self
                .client
                .head_object()
                .bucket(&self.bucket)
                .key(&src_name)
                .send()
                .await
                .is_ok();

            if is_file {
                let copy_source = url_encode(&format!("{}/{}", &self.bucket, &src_name));
                self.client
                    .copy_object()
                    .bucket(&self.bucket)
                    .copy_source(copy_source)
                    .key(&dst_name)
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::error!("Copy failed: {}", e);
                        FsError::GeneralFailure
                    })?;

                return Ok(());
            }

            let is_dir = self.is_dir(&src_name).await;
            if !is_dir || src_name.is_empty() {
                return Err(FsError::NotFound);
            }

            let prefix = format!("{}/", src_name);

            let objects = self.list_objects_by_prefix(&prefix).await;

            for obj in &objects {
                let old_key = obj.key().unwrap_or_default();
                let new_key = old_key.replacen(&src_name, &dst_name, 1);

                let copy_source = url_encode(&format!("{}/{}", &self.bucket, old_key));
                self.client
                    .copy_object()
                    .bucket(&self.bucket)
                    .copy_source(copy_source)
                    .key(&new_key)
                    .send()
                    .await
                    .map_err(|e| {
                        tracing::error!("Copy failed for {}: {}", old_key, e);
                        FsError::GeneralFailure
                    })?;
            }

            tracing::info!("Copy success");
            Ok(())
        })
    }
}
