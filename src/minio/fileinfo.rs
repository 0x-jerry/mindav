use std::fmt;
use std::time::SystemTime;

use dav_server::fs::{DavMetaData, FsResult};

#[derive(Clone)]
pub struct MinioMetaData {
    pub key: String,
    pub size: u64,
    pub last_modified: SystemTime,
    pub is_dir: bool,
}

impl fmt::Debug for MinioMetaData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MinioMetaData")
            .field("key", &self.key)
            .field("size", &self.size)
            .field("last_modified", &self.last_modified)
            .field("is_dir", &self.is_dir)
            .finish()
    }
}

impl MinioMetaData {
    pub fn new_dir(key: String) -> Self {
        MinioMetaData {
            key,
            size: 0,
            last_modified: SystemTime::now(),
            is_dir: true,
        }
    }
}

impl DavMetaData for MinioMetaData {
    fn len(&self) -> u64 {
        self.size
    }

    fn modified(&self) -> FsResult<SystemTime> {
        Ok(self.last_modified)
    }

    fn is_dir(&self) -> bool {
        self.is_dir
    }
}
