use anyhow::Result;
use colored::Colorize;

use crate::{config::Config, file::Metadata, git::Repo, paths::Paths};

use super::Runnable;

pub struct StatusOp;

impl Runnable for StatusOp {
    fn run(&self, _config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let status_changes = match repo.status_changes() {
            Ok(Some(status_changes)) => status_changes,
            Ok(None) => {
                tracing::trace!("no status change found");
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

        println!("{}", "Unsaved changes:".bold());

        for (status_type, file_data) in formatted_changes.iter() {
            println!(
                "{}: {}",
                status_type.into_colored_string(),
                file_data.unwrap().system_path.display()
            )
        }

        Ok(())
    }
}
