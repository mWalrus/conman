use std::path::PathBuf;

use anyhow::Result;
use dialoguer::{theme::ColorfulTheme, FuzzySelect};

use crate::{
    config::Config,
    file::{self, Metadata},
    paths::Paths,
};

use super::Runnable;

pub struct EditOp {
    pub path: Option<PathBuf>,
    pub skip_update: bool,
}

impl Runnable for EditOp {
    fn run(&self, config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let metadata = Metadata::read(&paths.metadata)?;

        let maybe_file_data = match &self.path {
            Some(path) => metadata.get_file_data_by_system_path(&path),
            None => {
                let theme = ColorfulTheme::default();
                let mut fuzzy_select = FuzzySelect::with_theme(&theme)
                    .default(0)
                    .with_prompt("Search for a file to edit");

                for file in metadata.files.iter() {
                    fuzzy_select = fuzzy_select.item(file.system_path.to_string_lossy());
                }

                let selected_index = fuzzy_select.interact()?;

                metadata.get_file_data_by_index(selected_index)
            }
        };

        let Some(file_data) = maybe_file_data else {
            return Ok(());
        };

        edit::edit_file(&file_data.system_path)?;

        tracing::trace!("user done editing");

        if self.skip_update {
            tracing::trace!("skipping updating internal copy of the file");
            return Ok(());
        }

        let source_was_updated =
            file::source_was_updated(&file_data.system_path, &file_data.repo_path)?;
        tracing::Span::current().record("source_was_updated", source_was_updated);

        if !source_was_updated {
            tracing::trace!("skipping copy of identical files");
            return Ok(());
        }

        file::copy_from_system(file_data, &config.encryption.passphrase)?;
        Ok(())
    }
}
