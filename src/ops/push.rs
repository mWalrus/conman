use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, git::Repo, paths::Paths, report};

use super::{Message, Runnable};

pub struct PushOp;

impl Runnable for PushOp {
    fn run(&self, config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        if repo.check_has_unsaved()? {
            report!(sender, "save or discard any unsaved changes first");
            return Ok(());
        }

        report!(sender, "pushing content...");
        repo.push(&config, &config.upstream.branch)?;
        report!(sender, "done!");

        Ok(())
    }
}
