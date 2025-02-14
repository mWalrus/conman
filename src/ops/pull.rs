use std::fmt::Display;

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, git::Repo, paths::Paths, report};

use super::Runnable;

pub struct PullOp;

impl Runnable for PullOp {
    fn run(
        &self,
        config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
        let repo = Repo::open(&paths)?;

        if repo.check_has_unsaved()? {
            report!(sender, "save or discard any unsaved changes first!");
            return Ok(());
        }

        report!(sender, "fetching content...");
        repo.pull(&config)?;
        report!(sender, "done!");

        Ok(())
    }
}
