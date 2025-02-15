use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Result;
use directories::BaseDirs;
use tracing::instrument;

pub(crate) const APPLICATION_NAME: &str = "conman";
pub(crate) const METADATA_FILE_NAME: &str = "_conman_internal_metadata.toml";
pub(crate) const METADATA_CACHE_FILE_NAME: &str = "_metadata_cache.toml";
pub(crate) const REPO_DIRECTORY: &str = "_conman_repo";

#[derive(Clone)]
pub struct Paths {
    pub repo: PathBuf,
    pub metadata: PathBuf,
    pub metadata_cache: PathBuf,
}

impl Paths {
    pub fn new() -> Result<Self> {
        // NOTE: if either of the below fallible operations fail, something unrelated to conman
        //       is wrong and we have to panic

        // SEE: https://docs.rs/directories/latest/directories/struct.BaseDirs.html#method.new
        let base_dirs = BaseDirs::new().unwrap();

        let cache = base_dirs.data_dir().join(APPLICATION_NAME);

        let repo = cache.join(REPO_DIRECTORY);
        let metadata_cache = cache.join(METADATA_CACHE_FILE_NAME);

        let metadata = repo.join(METADATA_FILE_NAME);

        Ok(Self {
            repo,
            metadata,
            metadata_cache,
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

/// create all conman related paths on the system
#[instrument]
pub fn create_dirs() -> Result<()> {
    let base_dirs = BaseDirs::new().unwrap();

    let repo = base_dirs.data_dir().join(APPLICATION_NAME);

    if !fs::exists(&repo)? {
        fs::create_dir(&repo)?;
        tracing::trace!("created $HOME/.local/share/{APPLICATION_NAME}");
    }

    let config = base_dirs.config_dir().join(APPLICATION_NAME);
    if !std::fs::exists(&config)? {
        std::fs::create_dir(&config)?;
        tracing::trace!("created $HOME/.config/{APPLICATION_NAME}");
    }

    Ok(())
}
