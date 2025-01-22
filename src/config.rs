use std::{fmt::Debug, fs::File, io::Read, path::PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::directories::DIRECTORIES;

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
    key_file: Option<PathBuf>,
    #[serde(default = "default_branch")]
    branch: String,
}

fn default_branch() -> String {
    "main".into()
}

impl Config {
    pub fn read() -> Result<Self> {
        let config_file = DIRECTORIES.config.join("config.toml");

        let mut config_file = File::open(config_file)?;

        let mut contents = String::new();
        config_file.read_to_string(&mut contents)?;

        let mut config: Config = toml::de::from_str(&contents)?;

        config.resolve_ssh_key_file();

        tracing::trace!("read config");
        Ok(config)
    }

    fn resolve_ssh_key_file(&mut self) {
        if let Some(key_file_path) = self.upstream.key_file.take() {
            let resolved_path = if key_file_path.is_relative() {
                Some(DIRECTORIES.ssh.join(key_file_path))
            } else if key_file_path.is_absolute() {
                Some(key_file_path)
            } else {
                None
            };
            self.upstream.key_file = resolved_path;

            tracing::trace!(
                path = ?self.upstream.key_file,
                "upstream ssh key path resolved",
            );
        }
    }
}
