use std::path::Path;

use serde::Deserialize;

#[derive(Default, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub host: HostConfig,
}

#[derive(Deserialize)]
pub struct ServerConfig {
    pub address: String,
    pub port: u16,
}

#[derive(Deserialize)]
pub struct HostConfig {
    pub tps: u64,
    pub max_clients: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".to_string(),
            port: 8080,
        }
    }
}

impl Default for HostConfig {
    fn default() -> Self {
        Self {
            tps: 60,
            max_clients: 64,
        }
    }
}

pub fn load_config(path: Option<&Path>) -> AppConfig {
    let config_str = std::fs::read_to_string(path.unwrap_or(Path::new("config.toml")));
    config_str
        .ok()
        .and_then(|s| toml::from_str(&s).ok())
        .unwrap_or_default()
}
