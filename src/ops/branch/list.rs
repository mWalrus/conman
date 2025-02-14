use std::fmt::Display;

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, git::Repo, ops::Runnable, paths::Paths, report};

pub struct ListOp;

impl Runnable for ListOp {
    fn run(
        &self,
        config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let branch_names = repo.local_branch_names()?;

        for branch in branch_names.into_iter() {
            if branch == config.upstream.branch {
                report!(sender, "{} (current)", branch);
            } else {
                report!(sender, branch);
            }
        }

        Ok(())
    }
}
