use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{
    config::Config,
    ops::{Message, Runnable},
    paths::Paths,
    report,
};

pub struct CurrentOp;

impl Runnable for CurrentOp {
    fn run(&self, config: Config, _paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        report!(sender, config.upstream.branch);
        Ok(())
    }
}
