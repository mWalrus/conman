use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use directories::BaseDirs;
use tracing::instrument;
use url_parse::core::Parser;

use crate::config::Config;

pub(crate) const APPLICATION_NAME: &str = "conman";
pub(crate) const METADATA_FILE_NAME: &str = "_conman_internal_metadata.toml";

pub struct Paths {
    pub repo: PathBuf,
    pub cache: PathBuf,
    pub metadata: PathBuf,
}

impl Paths {
    #[instrument(skip(config))]
    pub fn new(config: &Config) -> Result<Self> {
        // NOTE: if either of the below fallible operations fail, something unrelated to conman
        //       is wrong and we have to panic

        // SEE: https://docs.rs/directories/latest/directories/struct.BaseDirs.html#method.new
        let base_dirs = BaseDirs::new().unwrap();

        let cache = base_dirs.data_dir().join(APPLICATION_NAME);
        if !fs::exists(&cache).unwrap() {
            fs::create_dir(&cache).unwrap();
            tracing::trace!("created $HOME/.local/share/{APPLICATION_NAME}");
        }

        let url = Parser::new(None).parse(&config.upstream.url)?;

        let repo_name = url.path.unwrap().last().unwrap().clone();
        let repo = cache.join(repo_name);

        let metadata = repo.join(METADATA_FILE_NAME);

        Ok(Self {
            cache,
            repo,
            metadata,
        })
    }

    pub fn repo_local_file_path(&self, on_disk_path: &PathBuf) -> Result<PathBuf> {
        let file_name = on_disk_path.file_name().unwrap().to_string_lossy();

        let start = SystemTime::now();
        let timestamp = start.duration_since(UNIX_EPOCH)?.as_secs();

        let name = format!("{timestamp}-{file_name}");

        let path = self.repo.join(name);
        Ok(path)
    }
}
