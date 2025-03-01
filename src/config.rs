use std::{fmt::Debug, fs::File, io::Read, path::PathBuf};

use crate::paths::APPLICATION_NAME;
use anyhow::Result;
use directories::BaseDirs;
use serde::{Deserialize, Deserializer, Serialize};
use tracing::instrument;

const CONFIG_FILE_NAME: &str = "config.toml";

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Config {
    pub encryption: EncryptionConfig,
    pub upstream: UpstreamConfig,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EncryptionConfig {
    pub passphrase: String, // we will use this together with `age` for file encryption
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpstreamConfig {
    pub url: String,
    #[serde(default, deserialize_with = "path_resolver")]
    pub key_file: Option<PathBuf>,
    #[serde(default = "default_branch")]
    pub branch: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            encryption: EncryptionConfig {
                passphrase: String::new(),
            },
            upstream: UpstreamConfig {
                url: String::new(),
                key_file: None,
                branch: String::new(),
            },
        }
    }
}

#[inline(always)]
pub fn default_branch() -> String {
    "main".into()
}

impl Config {
    #[instrument]
    pub fn read() -> Result<Self> {
        let config_file = Self::config_file();

        let mut config_file = File::open(&config_file)?;

        let mut contents = String::new();
        config_file.read_to_string(&mut contents)?;

        let config: Config = toml::de::from_str(&contents)?;

        tracing::trace!("read config");
        Ok(config)
    }

    #[instrument(skip(self))]
    pub fn write(&self) -> Result<()> {
        let config_file = Self::config_file();

        let toml = toml::to_string(&self)?;
        tracing::trace!("serialized config");

        std::fs::write(&config_file, toml)?;
        tracing::trace!("wrote config to {}", config_file.display());

        Ok(())
    }

    #[instrument]
    pub fn write_default_config() -> Result<()> {
        let config_file = Self::config_file();

        println!("Config file not found, creating default...");
        let default_config = Self::default();

        let toml = toml::to_string(&default_config)?;
        tracing::trace!("serialized default config");

        std::fs::write(&config_file, toml)?;

        println!(
            "Wrote empty config to '{}'. Please populate it!",
            config_file.display()
        );

        Ok(())
    }

    fn config_file() -> PathBuf {
        let base_dirs = BaseDirs::new().unwrap();
        base_dirs
            .config_dir()
            .join(APPLICATION_NAME)
            .join(CONFIG_FILE_NAME)
    }
}

#[instrument(skip(de))]
fn path_resolver<'de, D>(de: D) -> Result<Option<PathBuf>, D::Error>
where
    D: Deserializer<'de>,
{
    let maybe_path: Option<PathBuf> = Option::deserialize(de)?;
    let Some(unresolved_path) = maybe_path else {
        tracing::trace!("no ssh key file path specified");
        return Ok(None);
    };

    tracing::trace!(unresolved_path=?unresolved_path, "got key file path");

    let Some(unresolved_path_as_str) = unresolved_path.to_str() else {
        tracing::warn!("unresolved path could not be converted to str");
        return Ok(None);
    };

    // FIXME: allow for expansion with env as well in order to support $HOME prefix
    let expanded_path = shellexpand::tilde(unresolved_path_as_str);
    tracing::trace!(expanded_path = ?expanded_path, "expanded key file path");

    let resolved_path = std::fs::canonicalize(expanded_path.into_owned()).unwrap();
    tracing::trace!(path=?resolved_path, "resolved ssh key file path");

    Ok(Some(resolved_path))
}
