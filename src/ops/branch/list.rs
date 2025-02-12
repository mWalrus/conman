use anyhow::Result;

use crate::{config::Config, git::Repo, ops::Runnable, paths::Paths};

pub struct ListOp;

impl Runnable for ListOp {
    fn run(&self, _config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let branch_names = repo.local_branch_names()?;

        println!("Branches:");
        for branch in branch_names.iter() {
            println!("  - {branch}");
        }

        Ok(())
    }
}
