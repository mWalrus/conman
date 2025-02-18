use anyhow::Result;
use crossbeam_channel::Sender;

use crate::{config::Config, file::Metadata, ops::Message, paths::Paths, report};

use super::Runnable;

pub struct ListOp;

impl Runnable for ListOp {
    fn run(&self, _config: Config, paths: Paths, sender: Option<Sender<Message>>) -> Result<()> {
        let metadata = Metadata::read(&paths.metadata)?;

        let encrypted_count = metadata.files.iter().filter(|file| file.encrypted).count();
        let non_encrypted_count = metadata.files.len() - encrypted_count;

        let encrypted = metadata.files.iter().filter(|file| file.encrypted);
        let non_encrypted = metadata.files.iter().filter(|file| !file.encrypted);

        if encrypted_count > 0 {
            report!(sender, "encrypted files:");
            for file in encrypted {
                report!(sender, "{}", file.system_path.display());
            }
        }

        if non_encrypted_count > 0 {
            report!(sender, "non-encrypted files:");
            for file in non_encrypted {
                report!(sender, "{}", file.system_path.display());
            }
        }

        Ok(())
    }
}
