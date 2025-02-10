// TODO: whenever we persist metadata to the internal conman repo,
// we should also persist this change to .local/share/conman/__branch_metadata

use std::{fs::File, io::Read, path::PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{file::FileData, state::STATE};

pub(crate) const BRANCH_CACHE_FILE_NAME: &str = "__branch_cache";

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct BranchCache {
    name: String,
    files: Vec<PathBuf>,
}

impl BranchCache {
    #[instrument]
    pub fn read() -> Result<Self> {
        let path = STATE.paths.cache.join(BRANCH_CACHE_FILE_NAME);

        let cache = match File::open(path) {
            Ok(mut file) => {
                tracing::trace!("found branch cache file");
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                tracing::trace!("read branch cache file");
                let branch_cache: BranchCache = toml::from_str(&contents)?;
                tracing::trace!("branch cache deserialized");
                branch_cache
            }
            Err(_) => {
                tracing::trace!("no branch cache found");
                BranchCache::default()
            }
        };

        Ok(cache)
    }

    pub fn is_empty(&self) -> bool {
        self.name.is_empty() && self.files.is_empty()
    }

    pub fn dangling_entries<'m>(&'m self, metadata: &'m Vec<FileData>) -> Vec<(PathBuf, bool)> {
        self.files
            .iter()
            .filter(|file| {
                metadata
                    .iter()
                    .find(|entry| entry.system_path.eq(*file))
                    .is_none()
            })
            .map(|file| {
                (
                    file.clone(),
                    metadata
                        .iter()
                        .find_map(|entry| entry.system_path.eq(file).then_some(entry.encrypted))
                        .unwrap_or(false),
                )
            })
            .collect()
    }

    #[instrument(skip(self, metadata))]
    pub fn update(&mut self, metadata: &Vec<FileData>) {
        self.name = STATE.config.upstream.branch.clone();
        self.files = metadata
            .clone()
            .into_iter()
            .map(|file| file.system_path)
            .collect();
        tracing::trace!("updated branch cache");
    }

    pub fn has_changes(&self, metadata: &Vec<FileData>) -> Result<bool> {
        let has_changes = self.files.iter().any(|file| {
            metadata
                .iter()
                .find(|entry| entry.system_path.eq(file))
                .is_none()
        });
        Ok(has_changes)
    }

    #[instrument(skip(self))]
    pub fn write(&self) -> Result<()> {
        let cache = toml::to_string(&self)?;
        tracing::trace!("serialized branch cache");

        let path = STATE.paths.cache.join(BRANCH_CACHE_FILE_NAME);
        std::fs::write(&path, cache)?;
        tracing::trace!("wrote cache to {}", path.display());

        Ok(())
    }
}
