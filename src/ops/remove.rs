use std::path::PathBuf;

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{
    config::Config,
    file::{self, Metadata},
    paths::Paths,
    report,
};

use super::{Message, Runnable};

pub struct RemoveOp {
    pub files: Vec<PathBuf>,
}

impl Runnable for RemoveOp {
    fn run(&self, _config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        if self.files.is_empty() {
            report!(sender, "No file(s) specified!");
            return Ok(());
        }

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
        file::write_cache(&metadata, &paths.metadata_cache)?;
        report!(sender, "done!");
        Ok(())
    }
}
