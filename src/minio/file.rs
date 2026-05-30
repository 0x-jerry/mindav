use std::io::SeekFrom;
use std::path::Path;

use aws_sdk_s3::primitives::ByteStream;
use bytes::{Buf, Bytes};
use dav_server::fs::{DavFile, DavMetaData, FsFuture, FsResult};
use md5::{Digest, Md5};

use super::fileinfo::MinioMetaData;

#[derive(Debug)]
pub struct MinioFile {
    client: aws_sdk_s3::Client,
    bucket: String,
    name: String,
    upload_mode: String,
    data: Vec<u8>,
    pos: u64,
    write_buf: Vec<u8>,
    is_open_for_write: bool,
    metadata: MinioMetaData,
}

impl MinioFile {
    pub async fn new_read(
        client: aws_sdk_s3::Client,
        bucket: String,
        name: String,
        upload_mode: String,
        metadata: MinioMetaData,
    ) -> FsResult<Self> {
        let result = client.get_object().bucket(&bucket).key(&name).send().await;

        let data = match result {
            Ok(output) => output
                .body
                .collect()
                .await
                .map(|d| d.to_vec())
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };

        Ok(MinioFile {
            client,
            bucket,
            name,
            upload_mode,
            data,
            pos: 0,
            write_buf: Vec::new(),
            is_open_for_write: false,
            metadata,
        })
    }

    pub fn new_write(
        client: aws_sdk_s3::Client,
        bucket: String,
        name: String,
        upload_mode: String,
        metadata: MinioMetaData,
    ) -> Self {
        MinioFile {
            client,
            bucket,
            name,
            upload_mode,
            data: Vec::new(),
            pos: 0,
            write_buf: Vec::new(),
            is_open_for_write: true,
            metadata,
        }
    }
}

impl DavFile for MinioFile {
    fn metadata(&mut self) -> FsFuture<'_, Box<dyn DavMetaData>> {
        let md = self.metadata.clone();
        Box::pin(async move { Ok(Box::new(md) as Box<dyn DavMetaData>) })
    }

    fn write_buf(&mut self, buf: Box<dyn Buf + Send>) -> FsFuture<'_, ()> {
        let mut buf = buf;
        let mut data = vec![0u8; buf.remaining()];
        buf.copy_to_slice(&mut data);
        self.write_buf.extend_from_slice(&data);
        Box::pin(async move { Ok(()) })
    }

    fn write_bytes(&mut self, buf: Bytes) -> FsFuture<'_, ()> {
        self.write_buf.extend_from_slice(&buf);
        Box::pin(async move { Ok(()) })
    }

    fn read_bytes(&mut self, count: usize) -> FsFuture<'_, Bytes> {
        let start = self.pos as usize;
        let end = usize::min(start + count, self.data.len());
        let result = if start < self.data.len() {
            Bytes::copy_from_slice(&self.data[start..end])
        } else {
            Bytes::new()
        };
        self.pos = end as u64;
        Box::pin(async move { Ok(result) })
    }

    fn seek(&mut self, pos: SeekFrom) -> FsFuture<'_, u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                let data_len = self.data.len() as i64;
                u64::max(0, (data_len + offset) as u64)
            }
            SeekFrom::Current(offset) => u64::max(0, (self.pos as i64 + offset) as u64),
        };
        self.pos = new_pos;
        Box::pin(async move { Ok(new_pos) })
    }

    fn flush(&mut self) -> FsFuture<'_, ()> {
        if !self.is_open_for_write {
            return Box::pin(async move { Ok(()) });
        }

        let client = self.client.clone();
        let bucket = self.bucket.clone();
        let name = self.name.clone();
        let upload_mode = self.upload_mode.clone();
        let data = std::mem::take(&mut self.write_buf);

        Box::pin(async move {
            if upload_mode == "memory" {
                let body = ByteStream::from(data);
                client
                    .put_object()
                    .bucket(&bucket)
                    .key(&name)
                    .body(body)
                    .content_type("application/octet-stream")
                    .send()
                    .await
                    .map_err(|_| dav_server::fs::FsError::GeneralFailure)?;
            } else {
                let md5_hash = format!("{:x}", Md5::digest(name.as_bytes()));
                let tmp_dir = Path::new("./tmp");
                tokio::fs::create_dir_all(tmp_dir)
                    .await
                    .map_err(|_| dav_server::fs::FsError::GeneralFailure)?;

                let tmp_path = tmp_dir.join(&md5_hash);

                tokio::fs::write(&tmp_path, &data)
                    .await
                    .map_err(|_| dav_server::fs::FsError::GeneralFailure)?;

                let body = ByteStream::from_path(&tmp_path)
                    .await
                    .map_err(|_| dav_server::fs::FsError::GeneralFailure)?;

                let result = client
                    .put_object()
                    .bucket(&bucket)
                    .key(&name)
                    .body(body)
                    .content_type("application/octet-stream")
                    .send()
                    .await;

                let _ = tokio::fs::remove_file(&tmp_path).await;

                result.map_err(|_| dav_server::fs::FsError::GeneralFailure)?;
            }

            Ok(())
        })
    }
}
