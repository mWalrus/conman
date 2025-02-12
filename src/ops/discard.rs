use std::path::PathBuf;

use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::{
    config::Config,
    file::{self, Metadata},
    git::{Repo, StatusType},
    paths::Paths,
};

use super::Runnable;

pub struct DiscardOp {
    pub files: Option<Vec<PathBuf>>,
    pub no_confirm: bool,
}

impl Runnable for DiscardOp {
    fn run(&self, config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let mut metadata = Metadata::read(&paths.metadata)?;

        let mut status_changes = match repo.status_changes() {
            Ok(Some(status_changes)) => status_changes,
            Ok(None) => {
                tracing::trace!("no status change found");
                return Ok(());
            }
            Err(e) => {
                return Err(e)?;
            }
        };

        let files = file::canonicalize_optional_paths(self.files.as_ref());

        if let Some(files) = files {
            status_changes.retain(|change| {
                let Some(file_data) =
                    metadata.get_file_data_where_repo_path_ends_with(&change.relative_path)
                else {
                    return false;
                };
                files.contains(&file_data.system_path)
            });
        }

        let mut confirmed_changes_to_reset = vec![];

        for change in status_changes.into_iter() {
            let maybe_file_data =
                metadata.get_file_data_where_repo_path_ends_with(&change.relative_path);

            let Some(file_data) = maybe_file_data else {
                continue;
            };

            let change_string = match change.status {
                StatusType::New => "new file",
                _ => "changes made to file",
            };

            if !self.no_confirm {
                let prompt = format!(
                    "Do you want to discard {} '{}'",
                    change_string,
                    file_data.system_path.display()
                );

                let confirmation = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .interact()?;

                if !confirmation {
                    continue;
                }
            }

            confirmed_changes_to_reset.push(change);
        }

        if !confirmed_changes_to_reset.is_empty() {
            repo.reset(&confirmed_changes_to_reset)?;
        }

        let mut should_persist_metadata = false;

        for change in confirmed_changes_to_reset.iter() {
            let maybe_file_data =
                metadata.get_file_data_where_repo_path_ends_with(&change.relative_path);

            let Some(file_data) = maybe_file_data else {
                continue;
            };

            match change.status {
                StatusType::New => {
                    // EXPLANATION: We have to clone because borrow checker
                    let system_path = file_data.system_path.clone();
                    metadata.unmanage_file(&system_path)?;
                    should_persist_metadata = true;
                }
                StatusType::Modified => {
                    file::copy_from_repo(file_data, &config.encryption.passphrase)?;
                }
                StatusType::Deleted => {
                    metadata.manage_file(file_data.clone());
                    should_persist_metadata = true;
                }
                _ => {}
            }
        }

        if should_persist_metadata {
            metadata.persist()?;
            file::write_cache(&metadata, &paths.metadata_cache)?;
        }

        Ok(())
    }
}
