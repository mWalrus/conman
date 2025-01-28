use std::{fmt::Debug, fs::File, io::Read, path::PathBuf};

use crate::paths::APPLICATION_NAME;
use anyhow::Result;
use directories::BaseDirs;
use serde::{Deserialize, Deserializer, Serialize};
use tracing::instrument;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub encryption: EncryptionConfig,
    pub upstream: UpstreamConfig,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EncryptionConfig {
    pub passphrase: String, // we will use this together with `age` for file encryption
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpstreamConfig {
    pub url: String,
    #[serde(deserialize_with = "path_resolver")]
    pub key_file: Option<PathBuf>,
    #[serde(default = "default_branch")]
    pub branch: String,
}

#[inline(always)]
pub fn default_branch() -> String {
    "main".into()
}

impl Config {
    #[instrument]
    pub fn read() -> Result<Self> {
        let base_dirs = BaseDirs::new().unwrap();

        let config = base_dirs.config_dir().join(APPLICATION_NAME);
        if !std::fs::exists(&config).unwrap() {
            std::fs::create_dir(&config).unwrap();
            tracing::trace!("created $HOME/.config/{APPLICATION_NAME}");
        }

        let config_file = config.join("config.toml");
        let mut config_file = File::open(config_file)?;

        let mut contents = String::new();
        config_file.read_to_string(&mut contents)?;

        let config: Config = toml::de::from_str(&contents)?;

        tracing::trace!("read config");
        Ok(config)
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

    let expanded_path = shellexpand::tilde(unresolved_path_as_str);
    tracing::trace!(expanded_path = ?expanded_path, "expanded key file path");

    let resolved_path = std::fs::canonicalize(expanded_path.into_owned()).unwrap();
    tracing::trace!(path=?resolved_path, "resolved ssh key file path");

    Ok(Some(resolved_path))
}
