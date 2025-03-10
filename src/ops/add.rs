use std::path::PathBuf;

use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{
    config::Config,
    file::{self, FileData, Metadata},
    paths::Paths,
    report,
};

use super::{Message, Runnable};

pub struct AddOp {
    pub files: Vec<PathBuf>,
    pub encrypt: bool,
}

impl Runnable for AddOp {
    fn run(&self, config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        if self.files.is_empty() {
            report!(sender, "No file(s) specified!");
            return Ok(());
        }

        let mut metadata = Metadata::read(&paths.metadata)?;

        let sources = file::canonicalize_paths(&self.files);

        for source in sources.into_iter() {
            report!(sender, "adding file '{}'", source.display());

            let source_path = std::fs::canonicalize(source)?;

            tracing::trace!(source=?source_path, "canonicalized source path");

            if metadata.file_is_already_managed(&source_path) {
                tracing::trace!("file is already managed, skipping");
                return Ok(());
            }

            let destination_path = paths.repo_local_file_path(&source_path)?;

            let file_data = FileData::new(source_path, destination_path, self.encrypt);

            file::copy_from_system(&file_data, &config.encryption.passphrase)?;

            metadata.manage_file(file_data);
        }

        metadata.persist()?;
        file::write_cache(&metadata, &paths.metadata_cache)?;

        report!(sender, "done!");
        Ok(())
    }
}
