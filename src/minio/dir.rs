use std::time::SystemTime;

use dav_server::fs::{DavDirEntry, DavMetaData, FsFuture};

use crate::minio::fileinfo::MinioMetaData;

pub struct MinioDirEntry {
    pub key: String,
    pub size: u64,
    pub last_modified: SystemTime,
    pub is_dir: bool,
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
