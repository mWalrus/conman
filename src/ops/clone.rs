use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, git::Repo, paths::Paths, report};

use super::{Message, Runnable};

pub struct CloneOp;

impl Runnable for CloneOp {
    fn run(&self, config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        report!(sender, "initializing...");
        Repo::clone(&paths, &config)?;
        report!(sender, "done!");
        Ok(())
    }
}
