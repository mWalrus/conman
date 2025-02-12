use anyhow::Result;

use crate::{config::Config, ops::Runnable, paths::Paths};

pub struct CurrentOp;

impl Runnable for CurrentOp {
    fn run(&self, config: Config, _paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        println!("current branch: {}", config.upstream.branch);
        Ok(())
    }
}
