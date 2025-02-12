use std::path::PathBuf;

use anyhow::Result;

use crate::{
    config::Config,
    file::{self, Metadata},
    paths::Paths,
};

use super::Runnable;

pub struct RemoveOp {
    pub files: Vec<PathBuf>,
}

impl Runnable for RemoveOp {
    fn run(&self, _config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let mut metadata = Metadata::read(&paths.metadata)?;

        let files = file::canonicalize_paths(&self.files);

        for file in files {
            let Some(file_data) = metadata.unmanage_file(&file)? else {
                return Ok(());
            };

            file::remove_from_repo(&file_data)?;
        }

        metadata.persist()?;
        Ok(())
    }
}
