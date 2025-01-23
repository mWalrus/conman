use std::path::PathBuf;

use anyhow::Result;
use git2::{build::RepoBuilder, Cred, FetchOptions, RemoteCallbacks, Repository};

use crate::{config::Config, directories::DIRECTORIES, file::FileManager};

pub struct Repo(Repository);

impl Repo {
    pub fn open(config: &Config) -> Result<Self> {
        let repo_path = DIRECTORIES.local_repo_path(config)?;

        tracing::trace!(path=?repo_path, "attempting to open repo");
        let repo = Repository::open(&repo_path).unwrap();
        tracing::trace!(path=?repo_path, "opened repo");

        Ok(Self(repo))
    }

    pub fn clone(config: &Config) -> Result<()> {
        // do nothing if we can successfully open the repo on disk since we
        // don't have to clone if that's the case
        let repo_path = DIRECTORIES.local_repo_path(config)?;

        if let Ok(true) = std::fs::exists(&repo_path) {
            tracing::trace!("repo path already exists");
            return Ok(());
        }

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            tracing::trace!("fetching credentials to use for clone from upstream");
            let username = username_from_url.unwrap();

            if let Some(key) = config.upstream.key_file.as_ref() {
                tracing::trace!(username = username, key = ?key, "built ssh credentials");
                Cred::ssh_key(username, None, key, None)
            } else {
                // no creds?
                tracing::trace!(
                    username = username,
                    "built username cred since no key file was found"
                );
                Cred::username(username)
            }
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch_options);

        let url = &config.upstream.url;

        tracing::trace!(url = url, "attempting to clone from upstream");
        let _ = builder.clone(url, &repo_path)?;
        tracing::trace!(url = url, "cloned repo from upstream");

        Ok(())
    }

    /// Add a file from your local system to be managed by conman
    pub fn add(&self, config: &Config, source: PathBuf, encrypt: bool) -> Result<()> {
        let source_path = std::fs::canonicalize(source)?;

        let mut file_manager = FileManager::new(config)?;

        // we return if the file is already managed since we
        // don't want to do anything in this case
        if file_manager.is_already_managed(&source_path) {
            return Ok(());
        }

        let local_repo_path = DIRECTORIES.local_repo_path(config)?;

        let file_name = source_path.file_name().unwrap();
        let destination_path = local_repo_path.join(file_name);

        file_manager.copy(config, &source_path, &destination_path, encrypt)?;

        Ok(())
    }
}
