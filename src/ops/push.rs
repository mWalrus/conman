use std::fmt::Display;

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, git::Repo, paths::Paths, report};

use super::Runnable;

pub struct PushOp;

impl Runnable for PushOp {
    fn run(
        &self,
        config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
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
