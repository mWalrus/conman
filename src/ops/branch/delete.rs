use anyhow::Result;
use crossbeam_channel::Sender;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::{
    config::Config,
    git::Repo,
    ops::{Message, Runnable},
    paths::Paths,
    report,
};

pub struct DeleteOp(pub String);

impl Runnable for DeleteOp {
    fn run(&self, _config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        let confirmation = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Are you sure you want to delete {}", &self.0))
            .interact()?;

        if confirmation {
            repo.delete_branch(&self.0)?;
            report!(sender, "deleted branch {}", &self.0);
        } else {
            report!(sender, "no branch deleted");
        }

        Ok(())
    }
}
