use anyhow::Result;

use crate::{config::Config, git::Repo, ops::Runnable, paths::Paths};

pub struct CheckoutOp(pub String);

impl Runnable for CheckoutOp {
    fn run(&self, mut config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        config.upstream.branch = self.0.clone();

        let repo = Repo::open(&paths)?;

        repo.checkout(&config.upstream.branch)?;
        repo.set_upstream(&config.upstream.branch)?;

        config.write()?;
        Ok(())
    }
}
