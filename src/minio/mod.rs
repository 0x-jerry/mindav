mod dir;
mod file;
mod fileinfo;
mod filesystem;

pub use filesystem::MinioFs;

pub const KEEP_FILE_NAME: &str = ".mindavkeep";
pub const KEEP_FILE_CONTENT_TYPE: &str = "application/mindav-folder-keeper";

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UploadMode {
    Memory,
    File,
}

#[derive(Debug, Clone)]
pub struct MinioFsConfig {
    pub endpoint: String,
    pub bucket_name: String,
    pub ssl: bool,
    pub access_key: String,
    pub secret_access_key: String,
    pub upload_mode: UploadMode,
}
