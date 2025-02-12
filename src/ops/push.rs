use anyhow::Result;

use crate::{config::Config, git::Repo, paths::Paths};

use super::Runnable;

pub struct PushOp;

impl Runnable for PushOp {
    fn run(&self, config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        if repo.check_has_unsaved()? {
            println!("save or discard unsaved changes first");
            return Ok(());
        }

        repo.push(&config, &config.upstream.branch)?;

        Ok(())
    }
}
