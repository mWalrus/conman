use std::{fs::File, io::Write, path::PathBuf};

use age::secrecy::SecretString;
use anyhow::Result;
use git2::{build::RepoBuilder, Cred, FetchOptions, RemoteCallbacks, Repository};

use crate::config::Config;

pub struct Repo(Repository);

impl Repo {
    pub fn open(config: &Config) -> Result<Self> {
        let repo_path = config.local_repo_path()?;

        tracing::trace!(path=?repo_path, "attempting to open repo");
        let repo = Repository::open(&repo_path).unwrap();
        tracing::trace!(path=?repo_path, "opened repo");

        Ok(Self(repo))
    }

    pub fn clone(config: &Config) -> Result<()> {
        // do nothing if we can successfully open the repo on disk since we
        // don't have to clone if that's the case
        let repo_path = config.local_repo_path()?;

        if let Ok(true) = std::fs::exists(&repo_path) {
            tracing::trace!("repo path already exists");
            return Ok(());
        }

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            tracing::trace!("fetching credentials to use for clone from upstream");
            let username = username_from_url.unwrap();

            if let Some(key) = config.ssh_key() {
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

        let url = config.upstream_url();
        tracing::trace!(url = url, "attempting to clone from upstream");
        let _ = builder.clone(&url, &repo_path)?;
        tracing::trace!(url = url, "cloned repo from upstream");

        Ok(())
    }

    /// Add a file from your local system to be managed by conman
    pub fn add(&self, config: &Config, source: PathBuf, encrypt: bool) -> Result<()> {
        // TODO: early return if the file is already added
        //       also report to the user that the file already is managed by conman
        let source_path = std::fs::canonicalize(source)?;
        let local_repo_path = config.local_repo_path()?;

        let file_name = source_path.file_name().unwrap();
        let destination_path = local_repo_path.join(file_name);

        // simply copy the contents of the source file to the destination
        // if no encryption is needed
        if !encrypt {
            tracing::trace!(source=?source_path, destination=?destination_path,"no encryption selected, performing simple file copy");
            std::fs::copy(source_path, destination_path)?;
            return Ok(());
        }

        tracing::trace!("preparing file encryption");

        let mut reader = File::open(&source_path)?;
        let mut file_contents: Vec<u8> = vec![];

        // copy the file contents to the above buffer
        std::io::copy(&mut reader, &mut file_contents)?;

        // initialize the encryptor
        let passphrase = SecretString::from(config.encryption_passphrase());
        let encryptor = age::Encryptor::with_user_passphrase(passphrase);

        // prepare the destination file
        let mut destination_file = File::create(&destination_path)?;

        tracing::trace!("encrypting");
        // write encrypted file contents to the destination file
        let mut writer = encryptor.wrap_output(&mut destination_file)?;
        writer.write_all(&file_contents)?;
        writer.finish()?;

        tracing::trace!(source=?source_path, destination=?destination_path, "copied and encrypted file contents");

        Ok(())
    }
}
