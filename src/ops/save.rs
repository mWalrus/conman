use std::fmt::Display;

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{
    config::Config,
    file::Metadata,
    git::{Repo, StatusChange},
    paths::Paths,
    report,
};

use super::Runnable;

pub struct SaveOp;

impl Runnable for SaveOp {
    fn run(
        &self,
        _config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let status_changes = match repo.status_changes() {
            Ok(Some(status_changes)) => status_changes,
            Ok(None) => {
                report!(sender, "no status change found, skipping save");
                return Ok(());
            }
            Err(e) => {
                return Err(e)?;
            }
        };

        let metadata = Metadata::read(&paths.metadata)?;

        let commit_message = construct_commit_message(&metadata, status_changes);
        repo.commit_changes(commit_message)?;

        report!(sender, "saved!");

        Ok(())
    }
}

fn construct_commit_message(metadata: &Metadata, status_changes: Vec<StatusChange>) -> String {
    let mut commit_message = "system-update: updating files\n\n".to_string();
    let change_count = status_changes.len();

    for (i, change) in status_changes.into_iter().enumerate() {
        let maybe_file_data =
            metadata.get_file_data_where_repo_path_ends_with(&change.relative_path);

        let Some(file_data) = maybe_file_data else {
            continue;
        };

        let update = format!(
            "{}: {}{}",
            change.status.to_str(),
            file_data.system_path.display(),
            if i + 1 == change_count { "" } else { "\n" }
        );

        commit_message.push_str(&update);
    }
    commit_message
}
