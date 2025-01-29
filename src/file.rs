use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

use age::{secrecy::SecretString, Encryptor};
use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tracing::instrument;

use crate::state::STATE;

#[derive(Deserialize, Serialize, Debug)]
pub struct FileMetadata {
    #[serde(
        deserialize_with = "deserialize_system_path",
        serialize_with = "serialize_system_path"
    )]
    pub system_path: PathBuf,
    #[serde(
        deserialize_with = "deserialize_system_path",
        serialize_with = "serialize_system_path"
    )]
    pub repo_path: PathBuf,
    pub encrypted: bool,
}

pub struct FileManager {
    metadata: Metadata,
}

#[derive(Deserialize, Serialize, Debug, Default)]
struct Metadata {
    metadata: Vec<FileMetadata>,
}

impl FileManager {
    #[instrument]
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
    #[instrument(skip(self))]
    pub fn copy(&mut self, from: PathBuf, to: PathBuf, encrypt: bool) -> Result<()> {
        if encrypt {
            let passphrase = STATE.config.encryption.passphrase.clone();
            let encryptor = Self::init_encryptor(passphrase);
            self.copy_encrypted(encryptor, &from, &to)?;
        } else {
            self.copy_unencrypted(&from, &to)?;
        }
        self.manage_new_file(from, to, encrypt)?;
        self.persist_metadata()?;
        Ok(())
    }

    /// add metadata about the newly added file to the conman store
    #[instrument(skip(self))]
    fn manage_new_file(&mut self, from: PathBuf, to: PathBuf, encrypt: bool) -> Result<()> {
        let new_metadata = FileMetadata {
            system_path: from,
            repo_path: to,
            encrypted: encrypt,
        };
        self.metadata.metadata.push(new_metadata);
        Ok(())
    }

    /// check whether the source path is already managed by conman
    #[instrument(skip(self))]
    pub fn is_already_managed(&self, from: &PathBuf) -> bool {
        for managed_file in self.metadata.metadata.iter() {
            if managed_file.system_path.eq(from) {
                return true;
            }
        }
        return false;
    }

    /// perform a simple copy of the file at source into the local conman git repo
    #[instrument(skip(self))]
    fn copy_unencrypted(&self, from: &PathBuf, to: &PathBuf) -> Result<()> {
        tracing::trace!("no encryption selected, performing simple file copy");
        std::fs::copy(from, to)?;
        tracing::trace!("copied file contents");
        Ok(())
    }

    /// perform an encrypted copy of the file at source into the local conman git repo
    #[instrument(skip(self, encryptor))]
    fn copy_encrypted(&self, encryptor: Encryptor, from: &PathBuf, to: &PathBuf) -> Result<()> {
        tracing::trace!("preparing file copy with encryption");

        let mut reader = File::open(&from)?;
        let mut file_contents: Vec<u8> = vec![];

        // copy the file contents to the above buffer
        std::io::copy(&mut reader, &mut file_contents)?;

        // prepare the destination file
        let mut destination_file = File::create(&to)?;

        tracing::trace!("encrypting file contents");
        // write encrypted file contents to the destination file
        let mut writer = encryptor.wrap_output(&mut destination_file)?;
        writer.write_all(&file_contents)?;
        writer.finish()?;

        tracing::trace!("copied and encrypted file contents");

        Ok(())
    }

    /// persist file metadata to disk
    #[instrument(skip(self))]
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
    #[instrument(skip(self))]
    pub fn remove(&mut self, path: &PathBuf) -> Result<()> {
        let maybe_index = self
            .metadata
            .metadata
            .iter()
            .position(|managed_file| managed_file.system_path.eq(path));

        let Some(index) = maybe_index else {
            return Ok(());
        };

        tracing::trace!(index = index, "found index of path to remove");

        let removed_metadata = self.metadata.metadata.remove(index);
        tracing::trace!(remaining = ?self.metadata.metadata, "removed file metadata");

        // remove the file from the local git repo
        std::fs::remove_file(&removed_metadata.repo_path)?;
        tracing::trace!("removed file from repo");

        self.persist_metadata()?;

        tracing::trace!(removed=?removed_metadata, "removed managed file");

        Ok(())
    }
}

const USER_HOME_AMBIGUATION: &str = "__user_home__";

#[instrument(skip(de))]
fn deserialize_system_path<'de, D>(de: D) -> Result<PathBuf, D::Error>
where
    D: Deserializer<'de>,
{
    let mut path_string = String::deserialize(de)?;

    if path_string.starts_with(USER_HOME_AMBIGUATION) {
        let user_home = shellexpand::env("$HOME").unwrap();
        path_string = path_string.replace(USER_HOME_AMBIGUATION, &user_home);
    }

    let path = PathBuf::from(path_string);

    Ok(path)
}

fn serialize_system_path<S>(path: &PathBuf, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut path_string = path.to_string_lossy().to_string();

    let user_home = shellexpand::env("$HOME").unwrap().to_string();

    if path_string.starts_with(&user_home) {
        path_string = path_string.replace(&user_home, USER_HOME_AMBIGUATION);
    }

    ser.serialize_str(&path_string)
}
