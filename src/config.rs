use std::{fmt::Debug, fs::File, io::Read};

use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    encryption: EncryptionConfig,
    upstream: UpstreamConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EncryptionConfig {
    passphrase: String, // we will use this together with `age` for file encryption
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpstreamConfig {
    url: String,
    key: Option<String>,
    #[serde(default = "default_branch")]
    branch: String,
}

fn default_branch() -> String {
    "main".into()
}

impl Config {
    pub fn read() -> Result<Self> {
        let config_file = crate::DIRECTORIES.config.join("config.toml");

        let mut config_file = File::open(config_file)?;

        let mut contents = String::new();
        config_file.read_to_string(&mut contents)?;

        let config: Config = toml::de::from_str(&contents)?;

        tracing::trace!("read config");
        Ok(config)
    }
}
