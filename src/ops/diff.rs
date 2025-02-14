use std::fmt::Display;

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, paths::Paths, report};

use super::Runnable;

pub struct DiffOp;

impl Runnable for DiffOp {
    fn run(
        &self,
        _config: Config,
        _paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
        report!(sender, "not implemented");
        Ok(())
    }
}
