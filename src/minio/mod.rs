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
