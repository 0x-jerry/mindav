mod filesystem;
mod file;
mod fileinfo;

pub use filesystem::MinioFs;

pub const KEEP_FILE_NAME: &str = ".mindavkeep";
pub const KEEP_FILE_CONTENT_TYPE: &str = "application/mindav-folder-keeper";

pub fn clean_path_name(name: &str) -> String {
    let segments: Vec<&str> = name
        .split('/')
        .filter(|s| !s.is_empty() && *s != ".")
        .collect();

    let mut result: Vec<&str> = Vec::new();
    for seg in segments {
        if seg == ".." {
            result.pop();
        } else {
            result.push(seg);
        }
    }

    let cleaned = result.join("/");
    if cleaned.is_empty() {
        "/".to_string()
    } else {
        cleaned
    }
}
