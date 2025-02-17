use std::{fmt::Display, path::PathBuf};

use anyhow::Result;
use crossbeam_channel::Sender;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::{
    config::Config,
    file::{self, Metadata},
    git::Repo,
    paths::Paths,
    report,
};

use super::Runnable;

pub struct ApplyOp {
    pub files: Option<Vec<PathBuf>>,
    pub no_confirm: bool,
}

impl Runnable for ApplyOp {
    fn run(
        &self,
        config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
        let repo = Repo::open(&paths)?;

        if repo.check_has_unsaved()? {
            report!(sender, "save or discard unsaved changes first");
            return Ok(());
        }

        let mut metadata = Metadata::read(&paths.metadata)?;

        let maybe_files = file::canonicalize_optional_paths(self.files.as_ref());

        if let Some(files) = maybe_files {
            report!(sender, "preparing selected files");
            metadata
                .files
                .retain(|file| files.contains(&file.system_path));
        }

        for file_data in metadata.files.iter() {
            if !self.no_confirm {
                let prompt = format!("Do you want to apply '{}'", file_data.system_path.display());

                let confirmation = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .interact()?;

                if !confirmation {
                    continue;
                }
            }

            if let Some(parent) = file_data.system_path.parent() {
                if !parent.exists() {
                    tracing::trace!("parent(s) does not exist");
                    std::fs::create_dir_all(parent)?;
                    tracing::trace!("created parent dirs");
                }
            }

            file::copy_from_repo(file_data, &config.encryption.passphrase)?;
        }

        report!(sender, "done!");
        Ok(())
    }
}
