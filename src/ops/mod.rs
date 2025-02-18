use std::{fmt::Display, thread::JoinHandle};

use add::AddOp;
use anyhow::Result;
use apply::ApplyOp;
use clone::CloneOp;
use collect::CollectOp;
use crossbeam_channel::{Receiver, Sender};
use diff::DiffOp;
use discard::DiscardOp;
use edit::EditOp;
use list::ListOp;
use pull::PullOp;
use push::PushOp;
use remove::RemoveOp;
use save::SaveOp;
use status::StatusOp;
use verify_cache::VerifyCacheOp;

use crate::{
    args::{BranchCommand, Command},
    config::Config,
    paths::Paths,
};

pub mod add;
pub mod apply;
pub mod branch;
pub mod clone;
pub mod collect;
pub mod diff;
pub mod discard;
pub mod edit;
pub mod list;
pub mod pull;
pub mod push;
pub mod remove;
pub mod save;
pub mod status;
pub mod verify_cache;

type RunnableOperation = Box<dyn Runnable + Send + Sync>;
pub type Message = Box<dyn Display + Send + Sync>;

/// Convenience macro for sending progress reports through a given channel
#[macro_export]
macro_rules! report {
    ($sender:tt, $message:expr) => {
        if let Some(tx) = $sender.as_ref() {
            match tx.send(Box::new($message)) {
                Ok(()) => {},
                Err(e) => eprintln!("ERROR: {e:?}"),
            }
        }
    };
    ($sender:tt, $base:expr, $($arg:expr),*) => {
        report!($sender, format!($base, $($arg),*))
    };
}

pub trait Runnable {
    fn run(&self, config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()>;

    fn run_silent(&self, config: Config, paths: Paths) -> Result<()> {
        self.run(config, paths, None)
    }
}

/// An `Operation` is a runnable task constructed from a given command-line argument and their
/// parameters. It can be run in a blocking or non-blocking manner.
pub struct Operation {
    tx: Option<Sender<Message>>,
    inner: RunnableOperation,
    config: Config,
    paths: Paths,
}

impl Operation {
    /// construct an operation from a given cli arg
    pub fn new(command: Command) -> Result<Self> {
        let inner: RunnableOperation = match command {
            Command::Init => Box::new(CloneOp),
            Command::Branch { branch_op } => match branch_op {
                BranchCommand::Checkout { branch } => Box::new(branch::CheckoutOp(branch)),
                BranchCommand::List => Box::new(branch::ListOp),
                BranchCommand::Delete { branch } => Box::new(branch::DeleteOp(branch)),
                BranchCommand::Current => Box::new(branch::CurrentOp),
            },
            Command::Diff => Box::new(DiffOp),
            Command::Status => Box::new(StatusOp),
            Command::Edit { path, skip_update } => Box::new(EditOp { path, skip_update }),
            Command::Save => Box::new(SaveOp),
            Command::Push => Box::new(PushOp),
            Command::Pull => Box::new(PullOp),
            Command::Add { files, encrypt } => Box::new(AddOp { files, encrypt }),
            Command::List => Box::new(ListOp),
            Command::Remove { files } => Box::new(RemoveOp { files }),
            Command::Apply { files, no_confirm } => Box::new(ApplyOp { files, no_confirm }),
            Command::Discard { files, no_confirm } => Box::new(DiscardOp { files, no_confirm }),
            Command::Collect { files, no_confirm } => Box::new(CollectOp { files, no_confirm }),
        };

        let config = Config::read()?;
        let paths = Paths::new()?;

        Ok(Self {
            tx: None,
            inner,
            config,
            paths,
        })
    }

    /// create an `Operation` that will validate the current conman cache
    pub fn verify_cache() -> Result<Self> {
        let config = Config::read()?;
        let paths = Paths::new()?;

        Ok(Self {
            tx: None,
            inner: Box::new(VerifyCacheOp),
            config,
            paths,
        })
    }

    /// subscribe to progress updates from the current operation
    pub fn subscribe(&mut self) -> Receiver<Message> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.tx = Some(tx);
        rx
    }

    /// execute the operation in a separate thread
    pub fn execute(self) -> JoinHandle<Result<()>> {
        std::thread::spawn(move || self.inner.run(self.config, self.paths, self.tx))
    }

    /// execute the operation, blocking the main thread
    pub fn execute_blocking(self) -> Result<()> {
        self.inner.run_silent(self.config, self.paths)
    }
}

#[cfg(test)]
mod tests {
    use std::{fs::File, io::Write, path::PathBuf, sync::LazyLock};

    use crate::{
        file::Metadata,
        git::{Repo, StatusType},
        paths::{METADATA_CACHE_FILE_NAME, METADATA_FILE_NAME},
    };

    use super::*;
    use anyhow::Result;
    use rand::{distr::Alphanumeric, Rng};

    static TEST_PATH: LazyLock<PathBuf> =
        LazyLock::new(|| std::env::temp_dir().join("conman_test"));

    fn state() -> (Paths, Config) {
        let repo_dir_name: String = rand::rng()
            .sample_iter(Alphanumeric)
            .take(12)
            .map(char::from)
            .collect();

        let cache_file_name = format!("{repo_dir_name}{METADATA_CACHE_FILE_NAME}");
        let repo_path = TEST_PATH.join(repo_dir_name);

        let paths = Paths {
            metadata: repo_path.join(METADATA_FILE_NAME),
            repo: repo_path,
            metadata_cache: TEST_PATH.join(cache_file_name),
        };
        let config = Config {
            encryption: crate::config::EncryptionConfig {
                passphrase: "12345".into(),
            },
            ..Default::default()
        };

        (paths, config)
    }

    fn create_temp_file(name: &str) -> Result<PathBuf> {
        let path = TEST_PATH.join(name);
        let mut file = File::create(&path)?;

        let content = b"test content";
        let content_len = content.len();

        let written = file.write(content)?;

        assert_eq!(content_len, written);

        Ok(path)
    }

    fn add_files(files: Vec<&str>, encrypt: bool) -> (Paths, Config, Vec<PathBuf>) {
        let (paths, config) = state();

        Repo::create_at_path(&paths.repo);

        let files_len = files.len();

        let created_tmp_files: Vec<_> = files
            .into_iter()
            .map(|file| create_temp_file(file))
            .flatten()
            .collect();

        assert_eq!(files_len, created_tmp_files.len());

        let files = created_tmp_files
            .iter()
            .map(|file| PathBuf::from(file))
            .collect();

        AddOp { files, encrypt }
            .run(config.clone(), paths.clone(), None)
            .unwrap();

        (paths, config, created_tmp_files)
    }

    fn cleanup(paths: Paths, maybe_files: Option<Vec<PathBuf>>) {
        if let Some(files) = maybe_files {
            for file in files.into_iter() {
                if file.exists() {
                    std::fs::remove_file(file).unwrap();
                }
            }
        }

        if paths.metadata_cache.exists() {
            std::fs::remove_file(&paths.metadata_cache).unwrap();
        }
        if paths.repo.exists() {
            std::fs::remove_dir_all(&paths.repo).unwrap();
        }
    }

    #[test]
    fn create() {
        let repo_dir_name: String = rand::rng()
            .sample_iter(Alphanumeric)
            .take(12)
            .map(char::from)
            .collect();

        let repo_path = TEST_PATH.join(repo_dir_name);

        Repo::create_at_path(&repo_path);
        std::fs::remove_dir_all(&repo_path).unwrap();
    }

    #[test]
    fn add_unencrypted() {
        let (paths, _config, files) = add_files(vec!["add_unencrypted"], false);

        let repo = Repo::open(&paths).unwrap();
        let changes = repo.status_changes().unwrap();

        assert!(changes.is_some());

        let changes = changes.unwrap();

        assert!(changes.len() == 1);

        let change = &changes[0];
        assert!(change.status == StatusType::New);

        // NOTE: we have to read the metadata again after we execute the operation
        let metadata = Metadata::read(&paths.metadata).unwrap();
        assert!(!metadata.files.is_empty());

        for file in files.iter() {
            assert!(metadata.file_is_already_managed(&file));

            let file_data = metadata.get_file_data_by_system_path(&file).unwrap();

            assert!(!file_data.encrypted);
        }

        cleanup(paths, Some(files));
    }

    #[test]
    fn add_encrypted() {
        let (paths, _config, files) = add_files(vec!["add_encrypted"], true);

        let repo = Repo::open(&paths).unwrap();
        let changes = repo.status_changes().unwrap();

        assert!(changes.is_some());

        let changes = changes.unwrap();

        assert!(changes.len() == 1);

        let change = &changes[0];
        assert!(change.status == StatusType::New);

        // NOTE: we have to read the metadata again after we execute the operation
        let metadata = Metadata::read(&paths.metadata).unwrap();

        assert!(!metadata.files.is_empty());

        for file in files.iter() {
            assert!(metadata.file_is_already_managed(&file));

            let file_data = metadata.get_file_data_by_system_path(&file).unwrap();

            assert!(file_data.encrypted);
        }

        cleanup(paths, Some(files));
    }

    #[test]
    fn remove_file() {
        let (paths, config, files) = add_files(vec!["add_and_remove"], false);

        RemoveOp {
            files: files.clone(),
        }
        .run(config.clone(), paths.clone(), None)
        .unwrap();

        let metadata = Metadata::read(&paths.metadata).unwrap();

        for file in files.iter() {
            assert!(!metadata.file_is_already_managed(&file));
        }

        assert!(metadata.files.is_empty());

        cleanup(paths, Some(files));
    }

    #[test]
    fn remove_one_out_of_two_files() {
        let (paths, config, files) = add_files(
            vec![
                "remove_one_out_of_two_files_1",
                "remove_one_out_of_two_files_2",
            ],
            false,
        );

        let file_1 = files[0].clone();
        let file_2 = files[1].clone();

        RemoveOp {
            files: vec![file_1.clone()],
        }
        .run(config.clone(), paths.clone(), None)
        .unwrap();

        let metadata = Metadata::read(&paths.metadata).unwrap();

        assert!(!metadata.file_is_already_managed(&file_1));
        assert!(metadata.file_is_already_managed(&file_2));
        assert_eq!(metadata.files.len(), 1);

        cleanup(paths, Some(files));
    }

    #[test]
    fn edit_file_unencrypted() {
        let (paths, config, files) = add_files(vec!["edit_file_unencrypted"], false);

        // save to be able to discover modifications
        SaveOp.run(config.clone(), paths.clone(), None).unwrap();

        let metadata = Metadata::read(&paths.metadata).unwrap();
        assert!(metadata.file_is_already_managed(&files[0]));
        drop(metadata);

        // simulate edit
        let edit = b"we have made some epic changes to the file";
        std::fs::write(&files[0], edit).unwrap();

        CollectOp {
            files: None,
            no_confirm: true,
        }
        .run(config.clone(), paths.clone(), None)
        .unwrap();

        let repo = Repo::open(&paths).unwrap();

        let changes = repo.status_changes().unwrap();
        assert!(changes.is_some());

        let changes = changes.unwrap();
        assert!(changes.len() == 1);

        let metadata = Metadata::read(&paths.metadata).unwrap();
        let file_data = metadata.get_file_data_by_system_path(&files[0]);
        assert!(file_data.is_some());

        let file_data = file_data.unwrap();

        let content_in_file_after_collect = std::fs::read(&file_data.repo_path).unwrap();
        assert_eq!(edit, content_in_file_after_collect.as_slice());

        let change = &changes[0];
        assert!(change.status == StatusType::Modified);
        assert!(file_data.repo_path.ends_with(&change.relative_path));

        cleanup(paths, Some(files));
    }

    #[test]
    fn discard_changes() {
        let (paths, config, files) = add_files(vec!["discard_changes"], false);

        // save to be able to discover modifications
        SaveOp.run(config.clone(), paths.clone(), None).unwrap();

        let content_in_file_before_edit = std::fs::read(&files[0]).unwrap();

        let metadata = Metadata::read(&paths.metadata).unwrap();
        let file_data = metadata.get_file_data_by_system_path(&files[0]);
        assert!(file_data.is_some());

        let file_data = file_data.unwrap();

        // simulate edit to repo file
        let edit = b"we have made some epic changes to the file";
        std::fs::write(&file_data.repo_path, edit).unwrap();
        let content_in_file_after_edit = std::fs::read(&file_data.repo_path).unwrap();
        assert_eq!(edit, content_in_file_after_edit.as_slice());

        let repo = Repo::open(&paths).unwrap();

        let changes = repo.status_changes().unwrap();
        assert!(changes.is_some());

        let changes = changes.unwrap();
        assert!(changes.len() == 1);

        let change = &changes[0];
        assert!(change.status == StatusType::Modified);
        assert!(file_data.repo_path.ends_with(&change.relative_path));

        DiscardOp {
            files: Some(files.clone()),
            no_confirm: true,
        }
        .run(config.clone(), paths.clone(), None)
        .unwrap();

        let repo = Repo::open(&paths).unwrap();

        let changes = repo.status_changes().unwrap();
        assert!(changes.is_none());

        let content_in_file_after_discard = std::fs::read(&files[0]).unwrap();

        assert_eq!(content_in_file_before_edit, content_in_file_after_discard);

        cleanup(paths, Some(files));
    }

    #[test]
    fn apply() {
        let (paths, config, files) = add_files(vec!["apply_file"], false);

        SaveOp.run(config.clone(), paths.clone(), None).unwrap();

        let file = &files[0];

        let metadata = Metadata::read(&paths.metadata).unwrap();

        assert!(metadata.file_is_already_managed(file));

        let file_data = metadata.get_file_data_by_system_path(file);
        assert!(file_data.is_some());

        let file_data = file_data.unwrap();

        // simulate having change in repo but not on disk
        let edit = b"some edit content from apply_file";
        std::fs::write(&file_data.repo_path, edit).unwrap();

        SaveOp.run(config.clone(), paths.clone(), None).unwrap();

        let on_disk_content = std::fs::read(&file_data.system_path).unwrap();
        let in_repo_content = std::fs::read(&file_data.repo_path).unwrap();
        assert_ne!(in_repo_content.as_slice(), on_disk_content.as_slice());

        ApplyOp {
            files: None,
            no_confirm: true,
        }
        .run(config.clone(), paths.clone(), None)
        .unwrap();

        let on_disk_content_after_apply = std::fs::read(&file_data.system_path).unwrap();
        assert_eq!(
            in_repo_content.as_slice(),
            on_disk_content_after_apply.as_slice(),
        );

        cleanup(paths, Some(files));
    }
}
