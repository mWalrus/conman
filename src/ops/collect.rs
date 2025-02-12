use std::path::PathBuf;

use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::{
    config::Config,
    file::{self, Metadata},
    paths::Paths,
};

use super::Runnable;

pub struct CollectOp {
    pub files: Option<Vec<PathBuf>>,
    pub no_confirm: bool,
}

impl Runnable for CollectOp {
    fn run(&self, config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let mut metadata = Metadata::read(&paths.metadata)?;

        let maybe_files = file::canonicalize_optional_paths(self.files.as_ref());

        if let Some(files) = maybe_files {
            metadata
                .files
                .retain(|file| files.contains(&file.system_path));
        }

        for file in metadata.files.iter() {
            if !file::source_was_updated(&file.system_path, &file.repo_path)? {
                tracing::trace!("source has not been updated since last time");
                continue;
            }

            if self.no_confirm {
                file::copy_from_system(file, &config.encryption.passphrase)?;
                continue;
            }

            let message = format!("Collect updated file '{}'?", file.system_path.display());
            let confirmation = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(message)
                .interact()?;

            tracing::trace!("user gave confirmation: {confirmation}");
            if confirmation {
                file::copy_from_system(file, &config.encryption.passphrase)?;
            }
        }

        Ok(())
    }
}
