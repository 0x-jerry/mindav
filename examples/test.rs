use std::fs;

use aws_sdk_s3::config::{Credentials, Region};
use aws_sdk_s3::Client;

#[derive(serde::Deserialize)]
struct MinioConfig {
    endpoint: String,
    #[serde(rename = "bucketName")]
    bucket_name: String,
    ssl: bool,
    #[serde(rename = "accessKey")]
    access_key: String,
    #[serde(rename = "secretAccessKey")]
    secret_access_key: String,
}

#[derive(serde::Deserialize)]
struct Config {
    minio: MinioConfig,
}

#[tokio::main]
async fn main() {
    let config: Config = {
        let content = fs::read_to_string("config.json").expect("Failed to read config.json");
        serde_json::from_str(&content).expect("Failed to parse config.json")
    };

    let c = &config.minio;
    let scheme = if c.ssl { "https" } else { "http" };
    let endpoint_url = format!("{}://{}", scheme, c.endpoint);

    let credentials = Credentials::new(&c.access_key, &c.secret_access_key, None, None, "mindav");
    let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .region(Region::new("us-east-1"))
        .endpoint_url(&endpoint_url)
        .credentials_provider(credentials)
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&aws_config)
        .force_path_style(true)
        .build();
    let client = Client::from_conf(s3_config);

    println!("Listing objects in bucket '{}':", c.bucket_name);

    let result = client.list_objects_v2().bucket(&c.bucket_name).prefix("t/").delimiter("/").send().await;

    println!("result {:?}", result);

    if let Ok(r) = result {
        println!("key count: {:?}", r.key_count);
    }
}
