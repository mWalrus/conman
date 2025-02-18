use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, paths::Paths, report};

use super::{Message, Runnable};

pub struct DiffOp;

impl Runnable for DiffOp {
    fn run(&self, _config: Config, _paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        report!(sender, "not implemented");
        Ok(())
    }
}
