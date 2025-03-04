use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

use age::{secrecy::SecretString, Decryptor, Encryptor};
use anyhow::Result;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use tracing::instrument;

#[derive(Debug)]
pub enum CacheVerdict {
    FullPopulate(Metadata),
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

impl FileData {
    pub fn new(system_path: PathBuf, repo_path: PathBuf, encrypted: bool) -> Self {
        Self {
            system_path,
            repo_path,
            encrypted,
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct Metadata {
    #[serde(skip)]
    path: PathBuf,
    pub files: Vec<FileData>,
}

impl Metadata {
    #[instrument]
    pub fn read(path: &PathBuf) -> Result<Self> {
        let mut metadata = match File::open(&path) {
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

        metadata.path = path.clone();

        Ok(metadata)
    }

    pub fn get_file_data_by_index(&self, index: usize) -> Option<&FileData> {
        self.files.get(index)
    }

    pub fn get_file_data_by_system_path(&self, system_path: &PathBuf) -> Option<&FileData> {
        self.files
            .iter()
            .find(|file| file.system_path.eq(system_path))
    }

    pub fn get_file_data_where_repo_path_ends_with(&self, path: &PathBuf) -> Option<&FileData> {
        self.files
            .iter()
            .find(|file| file.repo_path.ends_with(path))
    }

    pub fn file_is_already_managed(&self, system_path: &PathBuf) -> bool {
        for managed_file in self.files.iter() {
            if managed_file.system_path.eq(system_path) {
                return true;
            }
        }
        return false;
    }

    /// manage the given `FileData`
    pub fn manage_file(&mut self, file_data: FileData) {
        self.files.push(file_data);
    }

    /// only remove the file from the internal metadata storage without removing the actual file
    /// from disk
    #[instrument(skip(self, system_path))]
    pub fn unmanage_file(&mut self, system_path: &PathBuf) -> Result<Option<FileData>> {
        let maybe_index = self
            .files
            .iter()
            .position(|file| file.system_path.eq(system_path));

        let Some(index) = maybe_index else {
            return Ok(None);
        };

        tracing::trace!(index = index, "found index of path to remove");

        Ok(Some(self.files.remove(index)))
    }

    #[instrument(skip(self))]
    pub fn persist(&self) -> Result<()> {
        let metadata = toml::to_string(self)?;

        std::fs::write(&self.path, metadata)?;
        tracing::trace!(path=?self.path, "wrote metadata to disk");

        Ok(())
    }
}

/// remove a managed file from the internal metadata storage and on disk
#[instrument(skip(file_data))]
pub fn remove_from_repo(file_data: &FileData) -> Result<()> {
    // remove the file from the local git repo
    match std::fs::remove_file(&file_data.repo_path) {
        Ok(()) => {
            tracing::trace!("removed file from repo");
        }
        Err(e) => {
            tracing::warn!("failed to remove repo file: {e}");
        }
    }

    tracing::trace!(removed=?file_data.repo_path, "removed managed file");
    Ok(())
}

/// read the current metadata and the cached metadata and compare the two returning
/// a verdict to action upon
#[instrument(skip(metadata_path, cache_path))]
pub fn verify_cache(metadata_path: &PathBuf, cache_path: &PathBuf) -> Result<CacheVerdict> {
    let cache = Metadata::read(cache_path)?;
    let metadata = Metadata::read(metadata_path)?;

    if cache.files.is_empty() && !metadata.files.is_empty() {
        return Ok(CacheVerdict::FullPopulate(metadata));
    }

    if cache.files.is_empty() && metadata.files.is_empty() {
        return Ok(CacheVerdict::DoNothing);
    }

    let dangling_files = dangling_cached_files(&metadata, &cache);

    if dangling_files.is_empty() {
        return Ok(CacheVerdict::DoNothing);
    }

    tracing::trace!("got {} dangling entries", dangling_files.len());

    Ok(CacheVerdict::HandleDangling(dangling_files))
}

fn dangling_cached_files(metadata: &Metadata, cache: &Metadata) -> Vec<FileData> {
    cache
        .files
        .iter()
        .filter(|theirs| {
            metadata
                .files
                .iter()
                .find(|ours| ours.system_path.eq(&theirs.system_path))
                .is_none()
        })
        .map(|theirs| theirs.clone())
        .collect()
}

/// writes the given metadata to the specified cache path
#[instrument(skip(metadata, cache_path))]
pub fn write_cache(metadata: &Metadata, cache_path: &PathBuf) -> Result<()> {
    let cache = toml::to_string(metadata)?;
    tracing::trace!("serialized branch cache");

    std::fs::write(cache_path, cache)?;
    tracing::trace!("wrote cache to {}", cache_path.display());

    Ok(())
}

/// performs a file content copy from a `FileData`'s `repo_path` to it's `system_path`
#[instrument(skip(file_data, passphrase))]
pub fn copy_from_repo(file_data: &FileData, passphrase: &str) -> Result<()> {
    if file_data.encrypted {
        copy_repo_encrypted(file_data, passphrase)?;
    } else {
        copy_any_unencrypted(&file_data.repo_path, &file_data.system_path)?;
    }
    Ok(())
}

/// performs a file content copy from a `FileData`'s encrypted `repo_path` to it's unencrypted `system_path`
#[instrument(skip(file_data, passphrase))]
pub fn copy_repo_encrypted(file_data: &FileData, passphrase: &str) -> Result<()> {
    let passphrase = SecretString::from(passphrase.to_string());

    let encrypted_file_contents = read_file_contents(&file_data.repo_path)?;

    let decryptor = Decryptor::new(&encrypted_file_contents[..])?;

    let mut decrypted_file_contents = vec![];
    let mut reader =
        decryptor.decrypt(std::iter::once(&age::scrypt::Identity::new(passphrase) as _))?;

    reader.read_to_end(&mut decrypted_file_contents)?;

    std::fs::write(&file_data.system_path, decrypted_file_contents)?;

    Ok(())
}

/// read the file contents at the provided `path`
#[instrument]
fn read_file_contents(path: &PathBuf) -> Result<Vec<u8>> {
    let mut reader = File::open(&path)?;
    let mut file_contents: Vec<u8> = vec![];

    std::io::copy(&mut reader, &mut file_contents)?;

    tracing::trace!("read file contents");
    Ok(file_contents)
}

/// set up the encryptor used for `age` file encryption
fn init_encryptor(secret: &str) -> Encryptor {
    let passphrase = SecretString::from(secret.to_string());
    Encryptor::with_user_passphrase(passphrase)
}

/// performs a file content copy from a `FileData`'s `system_path` to it's `repo_path`
#[instrument(skip(file_data, passphrase))]
pub fn copy_from_system(file_data: &FileData, passphrase: &str) -> Result<()> {
    if file_data.encrypted {
        let encryptor = init_encryptor(passphrase);
        copy_system_encrypted(encryptor, &file_data.system_path, &file_data.repo_path)?;
    } else {
        copy_any_unencrypted(&file_data.system_path, &file_data.repo_path)?;
    }
    Ok(())
}

/// perform a simple copy of the file at source into the local conman git repo
#[instrument]
fn copy_any_unencrypted(from: &PathBuf, to: &PathBuf) -> Result<()> {
    tracing::trace!("no encryption selected, performing simple file copy");
    std::fs::copy(from, to)?;
    tracing::trace!("copied file contents");
    Ok(())
}

/// perform an encrypted copy of the file at source into the local conman git repo
#[instrument(skip(encryptor))]
fn copy_system_encrypted(encryptor: Encryptor, from: &PathBuf, to: &PathBuf) -> Result<()> {
    tracing::trace!("preparing file copy with encryption");

    let file_contents = read_file_contents(&from)?;

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

/// Compares two files' metadata to check for differences
#[instrument(skip(source, dest))]
pub fn source_was_updated(source: &PathBuf, dest: &PathBuf) -> Result<bool> {
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

pub fn canonicalize_paths(files: &Vec<PathBuf>) -> Vec<PathBuf> {
    // FIXME: errors when canonicalizing non-existing paths
    files
        .into_iter()
        .map(|path| std::fs::canonicalize(path).unwrap())
        .collect()
}

pub fn canonicalize_optional_paths(maybe_files: Option<&Vec<PathBuf>>) -> Option<Vec<PathBuf>> {
    maybe_files.map(|files| canonicalize_paths(files))
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
