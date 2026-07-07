use serde::Deserialize;

use crate::minio::UploadMode;

fn default_upload_mode() -> UploadMode {
    UploadMode::Memory
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MinioConfig {
    pub endpoint: String,
    pub bucket_name: String,
    pub ssl: bool,
    pub access_key: String,
    pub secret_access_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AccountConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSection {
    pub port: String,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
    #[serde(default = "default_upload_mode")]
    pub upload_mode: UploadMode,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub app: AppSection,
    pub minio: MinioConfig,
}

impl Config {
    pub fn load() -> Self {
        let config_path = "config.json";
        let content = std::fs::read_to_string(config_path).unwrap_or_else(|_| {
            tracing::warn!("config.json not found, using defaults");
            String::from("{}")
        });

        let mut config: Config = serde_json::from_str(&content).unwrap_or_else(|e| {
            tracing::warn!("Failed to parse config.json: {}, using defaults", e);
            Config::default()
        });

        if config.app.port.is_empty() {
            config.app.port = "8080".to_string();
        }

        tracing::info!("Loaded config: {:?}", config);
        config
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            app: AppSection {
                port: "8080".to_string(),
                accounts: vec![],
                upload_mode: UploadMode::Memory,
            },
            minio: MinioConfig {
                endpoint: String::new(),
                bucket_name: String::new(),
                ssl: false,
                access_key: String::new(),
                secret_access_key: String::new(),
            },
        }
    }
}
