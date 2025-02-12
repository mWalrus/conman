use anyhow::Result;

use crate::{config::Config, git::Repo, paths::Paths};

use super::Runnable;

pub struct CloneOp;

impl Runnable for CloneOp {
    fn run(&self, config: Config, paths: Paths, report_fn: Box<dyn Fn(String)>) -> Result<()> {
        Repo::clone(&paths, &config)?;
        report_fn("Cloned repo".into());
        Ok(())
    }
}
