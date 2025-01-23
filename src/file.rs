use std::{
    fmt::Display,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

use age::{secrecy::SecretString, Encryptor};
use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::state::STATE;

#[derive(Deserialize, Serialize, Debug)]
pub struct FileMetadata {
    path: PathBuf,
    encrypted: bool,
}

impl Display for FileMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "File:\n\tpath: {:?}\n\tencrypted: {}",
            self.path, self.encrypted
        )
    }
}

pub struct FileManager {
    metadata: Metadata,
}

#[derive(Deserialize, Serialize, Debug, Default)]
struct Metadata {
    metadata: Vec<FileMetadata>,
}

// FIXME: ambiguate user specific paths so that we can install configs on systems with
//        differing user names.
//
//        Example:
//            - /home/wally/path/to/some/config.toml -> /home/<user>/path/to/some/config.toml
impl FileManager {
    pub fn new() -> Result<Self> {
        let metadata_path = &STATE.paths.metadata;

        let metadata = match File::open(&metadata_path) {
            Ok(mut file) => {
                tracing::trace!("found file metadata file");
                let mut contents = String::new();
                file.read_to_string(&mut contents)?;
                let metadata: Metadata = toml::from_str(&contents)?;
                tracing::trace!("done reading file metadata");
                metadata
            }
            Err(_) => {
                tracing::trace!("no file metadata file found");
                Metadata::default()
            }
        };

        Ok(Self { metadata })
    }

    /// set up the encryptor used for `age` file encryption
    fn init_encryptor(secret: String) -> Encryptor {
        let passphrase = SecretString::from(secret);
        Encryptor::with_user_passphrase(passphrase)
    }

    /// copy the file at `from` into `to`
    pub fn copy(&mut self, from: &PathBuf, to: &PathBuf, encrypt: bool) -> Result<()> {
        if encrypt {
            let passphrase = STATE.config.encryption.passphrase.clone();
            let encryptor = Self::init_encryptor(passphrase);
            self.copy_encrypted(encryptor, from, to)?;
        } else {
            self.copy_unencrypted(from, to)?;
        }
        self.manage_new_file(from, encrypt);
        self.persist_metadata()?;
        Ok(())
    }

    /// add metadata about the newly added file to the conman store
    fn manage_new_file(&mut self, from: &PathBuf, encrypt: bool) {
        let new_metadata = FileMetadata {
            path: from.clone(),
            encrypted: encrypt,
        };
        self.metadata.metadata.push(new_metadata);
    }

    /// check whether the source path is already managed by conman
    pub fn is_already_managed(&self, from: &PathBuf) -> bool {
        for managed_file in self.metadata.metadata.iter() {
            if managed_file.path.eq(from) {
                return true;
            }
        }
        return false;
    }

    /// perform a simple copy of the file at source into the local conman git repo
    fn copy_unencrypted(&self, from: &PathBuf, to: &PathBuf) -> Result<()> {
        tracing::trace!(source=?from, destination=?to,"no encryption selected, performing simple file copy");
        std::fs::copy(from, to)?;
        tracing::trace!(source=?from, destination=?to,"copied file contents");
        Ok(())
    }

    /// perform an encrypted copy of the file at source into the local conman git repo
    fn copy_encrypted(&self, encryptor: Encryptor, from: &PathBuf, to: &PathBuf) -> Result<()> {
        tracing::trace!(source=?from, "preparing file copy with encryption");

        let mut reader = File::open(&from)?;
        let mut file_contents: Vec<u8> = vec![];

        // copy the file contents to the above buffer
        std::io::copy(&mut reader, &mut file_contents)?;

        // prepare the destination file
        let mut destination_file = File::create(&to)?;

        tracing::trace!(source=?from, "encrypting file contents");
        // write encrypted file contents to the destination file
        let mut writer = encryptor.wrap_output(&mut destination_file)?;
        writer.write_all(&file_contents)?;
        writer.finish()?;

        tracing::trace!(source=?from, destination=?to, "copied and encrypted file contents");

        Ok(())
    }

    /// persist file metadata to disk
    fn persist_metadata(&self) -> Result<()> {
        let metadata = toml::to_string(&self.metadata)?;

        std::fs::write(&STATE.paths.metadata, metadata)?;
        tracing::trace!(path=?STATE.paths.metadata, "wrote metadata to disk");

        Ok(())
    }

    /// helper to access metadata
    pub fn metadata(&self) -> &Vec<FileMetadata> {
        &self.metadata.metadata
    }

    /// unmanage the file at the given path
    pub fn remove(&mut self, path: &PathBuf) -> Result<()> {
        let maybe_index = self
            .metadata
            .metadata
            .iter()
            .position(|managed_file| managed_file.path.eq(path));

        let Some(index) = maybe_index else {
            return Ok(());
        };

        let removed_metadata = self.metadata.metadata.remove(index);

        // remove the file from the local git repo
        let repo_local_path_to_removed_file =
            STATE.paths.repo_local_file_path(&removed_metadata.path);

        std::fs::remove_file(repo_local_path_to_removed_file)?;

        self.persist_metadata()?;

        tracing::trace!(removed=?removed_metadata, "removed metadata");

        Ok(())
    }
}
