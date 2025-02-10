use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

use age::{secrecy::SecretString, Encryptor};
use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tracing::instrument;

#[derive(Debug)]
pub enum CacheVerdict {
    FullPopulate,
    HandleDangling(Vec<FileData>),
    DoNothing,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct FileData {
    #[serde(
        deserialize_with = "deserialize_metadata_path",
        serialize_with = "serialize_metadata_path"
    )]
    pub system_path: PathBuf,
    #[serde(
        deserialize_with = "deserialize_metadata_path",
        serialize_with = "serialize_metadata_path"
    )]
    pub repo_path: PathBuf,
    pub encrypted: bool,
}

pub struct FileManager {
    metadata: Metadata,
}

#[derive(Deserialize, Serialize, Debug, Default)]
struct Metadata {
    files: Vec<FileData>,
}

impl FileManager {
    #[instrument]
    pub fn new(metadata_path: &PathBuf) -> Result<Self> {
        let metadata = Self::read_metadata(&metadata_path)?;
        Ok(Self { metadata })
    }

    fn read_metadata(path: &PathBuf) -> Result<Metadata> {
        let metadata = match File::open(&path) {
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
        Ok(metadata)
    }

    #[instrument(skip(self))]
    pub fn verify_cache(&self, cache_path: &PathBuf) -> Result<CacheVerdict> {
        let cache = Self::read_metadata(cache_path)?;

        if cache.files.is_empty() && !self.metadata.files.is_empty() {
            return Ok(CacheVerdict::FullPopulate);
        }

        if cache.files.is_empty() && self.metadata.files.is_empty() {
            return Ok(CacheVerdict::DoNothing);
        }

        let dangling_files = self.dangling_files(&cache);

        if dangling_files.is_empty() {
            return Ok(CacheVerdict::DoNothing);
        }

        tracing::trace!("got {} dangling entries", dangling_files.len());

        Ok(CacheVerdict::HandleDangling(dangling_files))
    }

    fn dangling_files(&self, other: &Metadata) -> Vec<FileData> {
        other
            .files
            .iter()
            .filter(|theirs| {
                self.metadata
                    .files
                    .iter()
                    .find(|ours| ours.system_path.eq(&theirs.system_path))
                    .is_none()
            })
            .map(|theirs| theirs.clone())
            .collect()
    }

    pub fn write_cache(&self, cache_path: &PathBuf) -> Result<()> {
        let cache = toml::to_string(&self.metadata)?;
        tracing::trace!("serialized branch cache");

        std::fs::write(cache_path, cache)?;
        tracing::trace!("wrote cache to {}", cache_path.display());

        Ok(())
    }

    /// set up the encryptor used for `age` file encryption
    fn init_encryptor(secret: String) -> Encryptor {
        let passphrase = SecretString::from(secret);
        Encryptor::with_user_passphrase(passphrase)
    }

    #[instrument(skip(self), fields(source_was_updated))]
    pub fn edit_managed_file(
        &self,
        path: Option<PathBuf>,
        skip_update: bool,
        passphrase: &str,
    ) -> Result<()> {
        let file_path = match path {
            Some(path) => path,
            None => {
                let theme = ColorfulTheme::default();
                let mut fuzzy_select = FuzzySelect::with_theme(&theme)
                    .default(0)
                    .with_prompt("Search for a file to edit");

                for file in self.metadata.files.iter() {
                    fuzzy_select = fuzzy_select.item(file.system_path.to_string_lossy());
                }

                let selected_index = fuzzy_select.interact()?;

                let selected = self
                    .metadata
                    .files
                    .get(selected_index)
                    .expect("failed to find just selected item somehow");

                PathBuf::from(&selected.system_path)
            }
        };

        tracing::trace!("got selected file path: {file_path:?}");

        let file_metadata = self
            .metadata
            .files
            .iter()
            .find(|file| file.system_path.eq(&file_path))
            .unwrap();

        tracing::trace!("found file with system path");

        edit::edit_file(&file_metadata.system_path)?;

        tracing::trace!("user done editing");

        if skip_update {
            tracing::trace!("skipping updating internal copy of the file");
            return Ok(());
        }

        let source_was_updated =
            self.source_was_updated(&file_metadata.system_path, &file_metadata.repo_path)?;
        tracing::Span::current().record("source_was_updated", source_was_updated);

        if !source_was_updated {
            tracing::trace!("skipping copy of identical files");
            return Ok(());
        }

        tracing::trace!("preparing to copy file contents");
        self.copy_managed_file(file_metadata, passphrase)?;
        Ok(())
    }

    fn copy_managed_file(&self, metadata: &FileData, passphrase: &str) -> Result<()> {
        if metadata.encrypted {
            let encryptor = Self::init_encryptor(passphrase.to_string());
            self.copy_encrypted(encryptor, &metadata.system_path, &metadata.repo_path)?;
        } else {
            self.copy_unencrypted(&metadata.system_path, &metadata.repo_path)?;
        }
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn collect(&self, path: Option<PathBuf>, no_confirm: bool, passphrase: &str) -> Result<()> {
        let mut file_metadata_collection = self.metadata.files.clone();
        if let Some(p) = path {
            file_metadata_collection.retain(|fm| fm.system_path.eq(&p));
        }

        for file in file_metadata_collection.iter() {
            let source_was_updated = self.source_was_updated(&file.system_path, &file.repo_path)?;
            if !source_was_updated {
                tracing::trace!("source in unchanged, skipping");
                continue;
            }

            if no_confirm {
                tracing::trace!("no confirmation needed, copying");
                self.copy_managed_file(file, passphrase)?;
                continue;
            }

            let message = format!("Collect updated file '{}'?", file.system_path.display());
            let confirmation = dialoguer::Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(message)
                .interact()?;

            tracing::trace!("user gave confirmation: {confirmation}");
            if confirmation {
                self.copy_managed_file(file, passphrase)?;
            }
        }

        Ok(())
    }

    /// Compares two files' metadata to check for differences
    #[instrument(skip(self, source, dest))]
    fn source_was_updated(&self, source: &PathBuf, dest: &PathBuf) -> Result<bool> {
        let source_metadata = std::fs::metadata(source)?;
        let dest_metadata = std::fs::metadata(dest)?;

        tracing::trace!(
            source = source_metadata.len(),
            dest = dest_metadata.len(),
            "measuring file sizes"
        );
        if source_metadata.len() != dest_metadata.len() {
            tracing::trace!("lengths do not match; files differ");
            return Ok(true);
        }

        let source_modified = source_metadata.modified()?;
        let dest_modified = dest_metadata.modified()?;

        tracing::trace!("checking modified time");
        if source_modified > dest_modified {
            tracing::trace!("source file was updated more recently");
            return Ok(true);
        }

        tracing::trace!("destination file is up-to-date");
        Ok(false)
    }

    /// manage a new file
    #[instrument(skip(self, dest_constructor))]
    pub fn manage(
        &mut self,
        from: PathBuf,
        encrypt: bool,
        dest_constructor: impl Fn(&PathBuf) -> Result<PathBuf>,
        passphrase: &str,
    ) -> Result<()> {
        let to = dest_constructor(&from)?;

        if encrypt {
            let encryptor = Self::init_encryptor(passphrase.to_string());
            self.copy_encrypted(encryptor, &from, &to)?;
        } else {
            self.copy_unencrypted(&from, &to)?;
        }
        self.add_file_to_metadata(from, to, encrypt)?;
        Ok(())
    }

    /// add metadata about the newly added file to the conman store
    #[instrument(skip(self))]
    fn add_file_to_metadata(&mut self, from: PathBuf, to: PathBuf, encrypt: bool) -> Result<()> {
        let new_metadata = FileData {
            system_path: from,
            repo_path: to,
            encrypted: encrypt,
        };
        self.metadata.files.push(new_metadata);
        Ok(())
    }

    /// check whether the source path is already managed by conman
    #[instrument(skip(self))]
    pub fn is_already_managed(&self, from: &PathBuf) -> bool {
        for managed_file in self.metadata.files.iter() {
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
    pub fn persist_metadata(&self, metadata_path: &PathBuf) -> Result<()> {
        let metadata = toml::to_string(&self.metadata)?;

        std::fs::write(metadata_path, metadata)?;
        tracing::trace!(path=?metadata_path, "wrote metadata to disk");

        Ok(())
    }

    /// helper to access metadata
    pub fn metadata(&self) -> &Vec<FileData> {
        &self.metadata.files
    }

    /// unmanage the file at the given path
    #[instrument(skip(self))]
    pub fn remove(&mut self, path: &PathBuf) -> Result<()> {
        let maybe_index = self
            .metadata
            .files
            .iter()
            .position(|managed_file| managed_file.system_path.eq(path));

        let Some(index) = maybe_index else {
            return Ok(());
        };

        tracing::trace!(index = index, "found index of path to remove");

        let removed_metadata = self.metadata.files.remove(index);
        tracing::trace!(remaining = ?self.metadata.files, "removed file metadata");

        // remove the file from the local git repo
        std::fs::remove_file(&removed_metadata.repo_path)?;
        tracing::trace!("removed file from repo");

        tracing::trace!(removed=?removed_metadata, "removed managed file");

        Ok(())
    }

    pub fn find_path(&self, file_name: &PathBuf) -> Option<&PathBuf> {
        self.metadata
            .files
            .iter()
            .find(|file| file.repo_path.ends_with(file_name.as_os_str()))
            .map(|file| &file.system_path)
    }
}

const USER_HOME_AMBIGUATION: &str = "__user_home__";

#[instrument(skip(de))]
fn deserialize_metadata_path<'de, D>(de: D) -> Result<PathBuf, D::Error>
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

fn serialize_metadata_path<S>(path: &PathBuf, ser: S) -> Result<S::Ok, S::Error>
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
