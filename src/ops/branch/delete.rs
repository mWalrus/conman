use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::{config::Config, git::Repo, ops::Runnable, paths::Paths};

pub struct DeleteOp(pub String);

impl Runnable for DeleteOp {
    fn run(&self, _config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let confirmation = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Are you sure you want to delete {}", &self.0))
            .interact()?;

        if confirmation {
            repo.delete_branch(&self.0)?;
        }

        Ok(())
    }
}
