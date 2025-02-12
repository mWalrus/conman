use anyhow::Result;
use colored::Colorize;
use dialoguer::theme::ColorfulTheme;

use crate::{
    config::Config,
    file::{self, CacheVerdict, Metadata},
    git::Repo,
    paths::Paths,
};

use super::Runnable;

pub struct VerifyCacheOp;

impl Runnable for VerifyCacheOp {
    fn run(&self, config: Config, paths: Paths, _report_fn: Box<dyn Fn(String)>) -> Result<()> {
        let Ok(repo) = Repo::open(&paths) else {
            return Ok(());
        };

        if !repo.head_matches(&config.upstream.branch).unwrap() {
            repo.checkout(&config.upstream.branch).unwrap();
            repo.set_upstream(&config.upstream.branch).unwrap();
        }

        let mut metadata = Metadata::read(&paths.metadata)?;

        let cache_verdict = file::verify_cache(&paths.metadata, &paths.metadata_cache)?;

        tracing::trace!("got cache verdict: {cache_verdict:?}");

        match cache_verdict {
            CacheVerdict::FullPopulate(metadata) => {
                file::write_cache(&metadata, &paths.metadata_cache)?
            }
            CacheVerdict::HandleDangling(dangling) => {
                println!(
                    "{}",
                    "Detected differences in managed files since last run!".bold()
                );

                let file_options = ["skip", "delete", "manage"];

                for file in dangling.into_iter() {
                    let choice = dialoguer::Select::with_theme(&ColorfulTheme::default())
                        .with_prompt(format!(
                            "Handle dangling file {}",
                            file.system_path.display()
                        ))
                        .items(&file_options)
                        .default(0)
                        .interact()?;

                    match file_options[choice] {
                        "delete" => {
                            std::fs::remove_file(&file.system_path)?;
                            tracing::trace!("deleted file {}", file.system_path.display());
                            println!("{}", "Deleted!".bold().green());
                        }
                        "manage" => {
                            file::copy_from_system(&file, &config.encryption.passphrase)?;
                            metadata.manage_file(file);
                        }
                        "skip" => {
                            tracing::trace!("skipping file");
                        }
                        _ => unreachable!(),
                    }
                }

                metadata.persist()?;
                file::write_cache(&metadata, &paths.metadata_cache)?;
            }
            CacheVerdict::DoNothing => {}
        };

        Ok(())
    }
}
