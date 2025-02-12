use anyhow::Result;
use colored::Colorize;

use crate::{config::Config, file::Metadata, paths::Paths};

use super::Runnable;

pub struct ListOp;

impl Runnable for ListOp {
    fn run(&self, _config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let metadata = Metadata::read(&paths.metadata)?;

        println!("{}", "Managed files:".bold());
        for (i, file) in metadata.files.iter().enumerate() {
            let encrypted_text = if file.encrypted {
                "true".green()
            } else {
                "false".red()
            };

            println!(
                "{} {}",
                "File".blue().bold(),
                (i + 1).to_string().blue().bold()
            );
            println!(
                "  {}: {}",
                "Path".bold(),
                file.system_path.display().to_string().underline()
            );
            println!("  {}: {}", "Encrypted".bold(), encrypted_text);
        }
        Ok(())
    }
}
