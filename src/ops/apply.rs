use std::path::PathBuf;

use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, Confirm};

use crate::{
    config::Config,
    file::{self, Metadata},
    git::Repo,
    paths::Paths,
};

use super::Runnable;

pub struct ApplyOp {
    pub files: Option<Vec<PathBuf>>,
    pub no_confirm: bool,
}

impl Runnable for ApplyOp {
    fn run(&self, config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let repo = Repo::open(&paths)?;

        if repo.check_has_unsaved()? {
            println!("save or discard unsaved changes first");
            return Ok(());
        }

        let mut metadata = Metadata::read(&paths.metadata)?;

        let maybe_files = file::canonicalize_optional_paths(self.files.as_ref());

        if let Some(files) = maybe_files {
            metadata
                .files
                .retain(|file| files.contains(&file.system_path));
        }

        for file_data in metadata.files.iter() {
            tracing::trace!(entry=?file_data.system_path ,"handling file");
            if !self.no_confirm {
                let prompt = format!("Do you want to apply '{}'", file_data.system_path.display());
                if let Ok(false) = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .interact()
                {
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

        Ok(())
    }
}
