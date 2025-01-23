use std::{fs, path::PathBuf, sync::LazyLock};

use anyhow::Result;
use directories::BaseDirs;
use url_parse::core::Parser;

use crate::config::Config;

pub(crate) const APPLICATION_NAME: &str = "conman";
pub const DIRECTORIES: LazyLock<Directories> = LazyLock::new(|| Directories::new());

pub struct Directories {
    pub cache: PathBuf,
    pub config: PathBuf,
}

impl Directories {
    fn new() -> Self {
        // NOTE: if either of the below fallible operations fail, something unrelated to conman
        //       is wrong and we have to panic

        // SEE: https://docs.rs/directories/latest/directories/struct.BaseDirs.html#method.new
        let base_dirs = BaseDirs::new().unwrap();

        let cache = base_dirs.data_dir().join(APPLICATION_NAME);
        if !fs::exists(&cache).unwrap() {
            fs::create_dir(&cache).unwrap();
            tracing::trace!("created $HOME/.local/share/{APPLICATION_NAME}");
        }

        let config = base_dirs.config_dir().join(APPLICATION_NAME);
        if !fs::exists(&config).unwrap() {
            fs::create_dir(&config).unwrap();
            tracing::trace!("created $HOME/.config/{APPLICATION_NAME}");
        }

        Self { cache, config }
    }

    pub fn local_repo_path(&self, config: &Config) -> Result<PathBuf> {
        let url = Parser::new(None).parse(&config.upstream.url)?;

        let repo_name = url.path.unwrap().last().unwrap().clone();
        let repo_path = self.cache.join(repo_name);

        Ok(repo_path)
    }

    pub fn metadata_path(&self, config: &Config) -> Result<PathBuf> {
        let local_repo_path = self.local_repo_path(config)?;
        let file_metadata_path = local_repo_path.join("metadata.toml");
        Ok(file_metadata_path)
    }

    pub fn config_path(&self) -> PathBuf {
        self.config.join("config.toml")
    }
}
