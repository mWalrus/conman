use anyhow::Result;

use crate::{config::Config, paths::Paths};

use super::Runnable;

pub struct DiffOp;

impl Runnable for DiffOp {
    fn run(&self, _config: Config, _paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        Ok(())
    }
}
