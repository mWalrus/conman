use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{
    config::Config,
    git::Repo,
    ops::{Message, Runnable},
    paths::Paths,
    report,
};

pub struct CheckoutOp(pub String);

impl Runnable for CheckoutOp {
    fn run(&self, mut config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        config.upstream.branch = self.0.clone();

        let repo = Repo::open(&paths)?;

        repo.checkout(&config.upstream.branch)?;
        repo.set_upstream(&config.upstream.branch)?;

        report!(sender, "checked out '{}'", &config.upstream.branch);

        config.write()?;

        report!(sender, "done!");

        Ok(())
    }
}
