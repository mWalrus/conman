use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, file::Metadata, git::Repo, paths::Paths, report};

use super::{Message, Runnable};

pub struct StatusOp;

impl Runnable for StatusOp {
    fn run(&self, _config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let status_changes = match repo.status_changes() {
            Ok(Some(status_changes)) => status_changes,
            Ok(None) => {
                report!(sender, "no changes found");
                return Ok(());
            }
            Err(e) => {
                return Err(e)?;
            }
        };

        let metadata = Metadata::read(&paths.metadata)?;

        let formatted_changes: Vec<_> = status_changes
            .into_iter()
            .map(|change| {
                (
                    change.status,
                    metadata.get_file_data_where_repo_path_ends_with(&change.relative_path),
                )
            })
            .filter(|(_, fd)| fd.is_some())
            .collect();

        report!(sender, "unsaved changes:");

        for (status_type, file_data) in formatted_changes.iter() {
            report!(
                sender,
                "{}: {}",
                status_type.to_str(),
                file_data.unwrap().system_path.display()
            )
        }

        Ok(())
    }
}
