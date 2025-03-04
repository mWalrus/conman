use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, git::Repo, paths::Paths, report};

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

        report!(sender, "unsaved changes:");

        for change in status_changes.iter() {
            report!(
                sender,
                "{}: {}",
                change.status.to_str(),
                change.relative_path.display()
            )
        }

        Ok(())
    }
}
