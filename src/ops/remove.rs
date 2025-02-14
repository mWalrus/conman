use std::{fmt::Display, path::PathBuf};

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{
    config::Config,
    file::{self, Metadata},
    paths::Paths,
    report,
};

use super::Runnable;

pub struct RemoveOp {
    pub files: Vec<PathBuf>,
}

impl Runnable for RemoveOp {
    fn run(
        &self,
        _config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()> {
        let mut metadata = Metadata::read(&paths.metadata)?;

        let files = file::canonicalize_paths(&self.files);

        for file in files {
            report!(sender, "removing file '{}'", file.display());

            let Some(file_data) = metadata.unmanage_file(&file)? else {
                return Ok(());
            };

            file::remove_from_repo(&file_data)?;
        }

        metadata.persist()?;
        report!(sender, "done!");
        Ok(())
    }
}
