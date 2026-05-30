#[allow(dead_code)]
pub fn env(key: &str, default_value: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default_value.to_string())
}
