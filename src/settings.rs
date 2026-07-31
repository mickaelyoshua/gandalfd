use config::{Config, Environment, File};
use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct AppSettings {
    pub port: u16,
    pub upstream_dns: Vec<String>,
    pub blocklist_urls: Vec<String>,
}

impl AppSettings {
    pub fn load() -> Self {
        let settings = Config::builder()
            .add_source(File::with_name("config.toml").required(false))
            .add_source(Environment::with_prefix("GANDALFD").separator("__"))
            .build()
            .expect("Failed to build configuration");

        settings
            .try_deserialize::<AppSettings>()
            .expect("Invalid configuration format")
    }
}
