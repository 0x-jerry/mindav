use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct MinioConfig {
    pub endpoint: String,
    #[serde(rename = "bucketName")]
    pub bucket_name: String,
    pub ssl: bool,
    #[serde(rename = "accessKey")]
    pub access_key: String,
    #[serde(rename = "secretAccessKey")]
    pub secret_access_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AdminConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppSection {
    pub port: String,
    pub admin: AdminConfig,
    #[serde(rename = "uploadMode")]
    pub upload_mode: String,
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
        if config.app.admin.username.is_empty() {
            config.app.admin.username = "admin".to_string();
        }
        if config.app.admin.password.is_empty() {
            config.app.admin.password = "password".to_string();
        }
        if config.app.upload_mode.is_empty() {
            config.app.upload_mode = "file".to_string();
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
                admin: AdminConfig {
                    username: "admin".to_string(),
                    password: "password".to_string(),
                },
                upload_mode: "file".to_string(),
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
