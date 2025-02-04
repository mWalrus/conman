// TODO: whenever we persist metadata to the internal conman repo,
// we should also persist this change to .local/share/conman/__branch_metadata

use std::{fs::File, io::Read, path::PathBuf};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::instrument;

use crate::{file::FileMetadata, state::STATE};

pub(crate) const BRANCH_CACHE_FILE_NAME: &str = "__branch_cache";

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct BranchCache {
    name: String,
    repo: String,
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
                branch_cache
            }
            Err(_) => {
                tracing::trace!("no branch cache found");
                BranchCache::default()
            }
        };

        Ok(cache)
    }

    #[instrument(skip(self, metadata), fields(cache, repo, equal))]
    pub fn has_changes(&self, metadata: Vec<FileMetadata>) -> Result<bool> {
        let has_changes = self.files.iter().all(|cache_file| {
            tracing::Span::current().record("cache", cache_file.to_str());
            metadata
                .iter()
                .find_map(|metadata_file| {
                    tracing::Span::current().record("repo", metadata_file.repo_path.to_str());
                    let equal = metadata_file.repo_path.eq(cache_file);
                    tracing::Span::current().record("equal", equal);
                    Some(equal)
                })
                .unwrap_or(false)
        });
        Ok(has_changes)
    }
}
