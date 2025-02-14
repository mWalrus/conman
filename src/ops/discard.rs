use std::{fmt::Display, path::PathBuf};

use anyhow::Result;
use crossbeam_channel::Sender;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::{
    config::Config,
    file::{self, Metadata},
    git::{Repo, StatusType},
    paths::Paths,
    report,
};

use super::Runnable;

pub struct DiscardOp {
    pub files: Option<Vec<PathBuf>>,
    pub no_confirm: bool,
}

impl Runnable for DiscardOp {
    fn run(
        &self,
        config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let mut metadata = Metadata::read(&paths.metadata)?;

        let mut status_changes = match repo.status_changes() {
            Ok(Some(status_changes)) => status_changes,
            Ok(None) => {
                report!(sender, "no changes found");
                return Ok(());
            }
            Err(e) => {
                return Err(e)?;
            }
        };

        let files = file::canonicalize_optional_paths(self.files.as_ref());

        if let Some(files) = files {
            report!(sender, "preparing selected files");
            status_changes.retain(|change| {
                let Some(file_data) =
                    metadata.get_file_data_where_repo_path_ends_with(&change.relative_path)
                else {
                    return false;
                };
                files.contains(&file_data.system_path)
            });
        }

        let files_to_reset: Vec<_> = status_changes
            .into_iter()
            .map(|change| {
                metadata
                    .get_file_data_where_repo_path_ends_with(&change.relative_path)
                    .map(|file| (change, file.clone()))
            })
            .flatten()
            .filter(|(_, file)| {
                if self.no_confirm {
                    return true;
                }

                let prompt = format!("do you want to discard '{}'", file.system_path.display());

                let confirmation = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .interact()
                    .unwrap();

                confirmation
            })
            .collect();

        if !files_to_reset.is_empty() {
            repo.reset(&files_to_reset)?;
        }

        let mut should_persist_metadata = false;

        for (change, file) in files_to_reset.into_iter() {
            report!(sender, "discarding file '{}'", file.system_path.display());

            match change.status {
                StatusType::New => {
                    // EXPLANATION: We have to clone because borrow checker
                    metadata.unmanage_file(&file.system_path)?;
                    should_persist_metadata = true;
                }
                StatusType::Modified => {
                    file::copy_from_repo(&file, &config.encryption.passphrase)?;
                }
                StatusType::Deleted => {
                    metadata.manage_file(file);
                    should_persist_metadata = true;
                }
                _ => {}
            }
        }

        if should_persist_metadata {
            metadata.persist()?;
            file::write_cache(&metadata, &paths.metadata_cache)?;
        }

        report!(sender, "done!");

        Ok(())
    }
}
