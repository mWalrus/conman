use std::path::PathBuf;

use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm};
use tracing::instrument;

use crate::{
    file::{CacheVerdict, FileManager},
    git::{Repo, StatusUpdate},
    state::STATE,
};

#[instrument]
pub fn init() -> Result<()> {
    Repo::clone()?;
    Ok(())
}

#[instrument(skip(_repo))]
pub fn diff(_repo: &Repo, _no_color: bool) -> Result<()> {
    Ok(())
}

#[instrument(skip(repo))]
pub fn status(repo: &Repo) -> Result<()> {
    let status_changes = match repo.status_changes() {
        Ok(Some(status_changes)) => status_changes,
        Ok(None) => {
            tracing::trace!("no status change found");
            return Ok(());
        }
        Err(e) => {
            return Err(e)?;
        }
    };

    print_status_updates(status_changes);

    Ok(())
}

fn print_status_updates(updates: Vec<StatusUpdate>) {
    println!("{}", "Unsaved changes:".bold());

    for update in updates.iter() {
        match (update.old.as_ref(), update.new.as_ref()) {
            (Some(old), Some(new)) if old != new => {
                println!(
                    "{}: {} -> {}",
                    update.status.into_colored_string(),
                    old.display(),
                    new.display()
                );
            }
            (old, new) => {
                println!(
                    "{}: {}",
                    update.status.into_colored_string(),
                    old.or(new).unwrap().display()
                )
            }
        }
    }
}

fn print_unsaved_changes_warning() {
    println!(
        "{}\n{}",
        "You have unsaved changes.".yellow(),
        "View them with `conman status` or save them with `conman save`"
    );
}

#[instrument(skip(repo))]
pub fn save(repo: &Repo) -> Result<()> {
    let status_changes = match repo.status_changes() {
        Ok(Some(status_changes)) => status_changes,
        Ok(None) => {
            tracing::trace!("no status change found, skipping save");
            return Ok(());
        }
        Err(e) => {
            return Err(e)?;
        }
    };

    let file_manager = FileManager::new()?;
    let commit_message = construct_commit_message(&file_manager, status_changes);
    repo.commit_changes(commit_message)?;

    Ok(())
}

#[instrument(skip(file_manager, status_changes))]
fn construct_commit_message(
    file_manager: &FileManager,
    status_changes: Vec<StatusUpdate>,
) -> String {
    // we need this to find the system path of each file
    let mut commit_message = "system-update: updating files\n\n".to_string();
    let change_count = status_changes.len();

    for (i, entry) in status_changes.into_iter().enumerate() {
        let file_path = entry.old.or(entry.new).unwrap();
        let Some(file_path) = file_manager.find_path(&file_path) else {
            continue;
        };

        let update = format!(
            "{}: {}{}",
            entry.status.to_str(),
            file_path.display(),
            if i + 1 == change_count { "" } else { "\n" }
        );

        commit_message.push_str(&update);
    }
    commit_message
}

#[instrument(skip(repo))]
pub fn pull(repo: &Repo) -> Result<()> {
    if repo.check_has_unsaved()? {
        print_unsaved_changes_warning();
        return Ok(());
    }

    repo.pull()?;

    Ok(())
}

#[instrument]
pub fn edit(path: Option<PathBuf>, skip_update: bool) -> Result<()> {
    FileManager::new()?.edit_managed_file(path, skip_update)?;
    Ok(())
}

#[instrument]
pub fn collect(path: Option<PathBuf>, no_confirm: bool) -> Result<()> {
    FileManager::new()?.collect(path, no_confirm)?;
    Ok(())
}

/// Add a file from your local system to be managed by conman
#[instrument]
pub fn add(source: PathBuf, encrypt: bool) -> Result<()> {
    let source_path = std::fs::canonicalize(source)?;

    tracing::trace!(source=?source_path, "canonicalized source path");

    let mut file_manager = FileManager::new()?;

    // we return if the file is already managed since we
    // don't want to do anything in this case
    if file_manager.is_already_managed(&source_path) {
        tracing::trace!("file is already managed, skipping");
        return Ok(());
    }

    file_manager.manage(source_path, encrypt)?;
    file_manager.persist_metadata()?;
    file_manager.write_cache()?;
    tracing::trace!("done adding file");

    Ok(())
}

#[instrument]
pub fn list() -> Result<()> {
    tracing::trace!("listing managed files");

    let file_manager = FileManager::new()?;
    let metadata = file_manager.metadata();

    println!("{}", "Managed files:".bold());
    for (i, file) in metadata.iter().enumerate() {
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

#[instrument]
pub fn remove(path: PathBuf) -> Result<()> {
    tracing::trace!(to_remove=?path, "removing managed file");
    let mut file_manager = FileManager::new()?;
    file_manager.remove(&path)?;
    Ok(())
}

#[instrument(skip(repo))]
pub fn apply(repo: &Repo, no_confirm: bool) -> Result<()> {
    if repo.check_has_unsaved()? {
        print_unsaved_changes_warning();
        return Ok(());
    }

    let file_manager = FileManager::new()?;

    for entry in file_manager.metadata() {
        tracing::trace!(entry=?entry ,"handling file");
        if !no_confirm {
            let prompt = format!("Do you want to apply '{}'", entry.system_path.display());
            if let Ok(false) = Confirm::with_theme(&ColorfulTheme::default())
                .with_prompt(prompt)
                .interact()
            {
                continue;
            }
        }

        if let Some(parent) = entry.system_path.parent() {
            if !parent.exists() {
                tracing::trace!("parent(s) does not exist");
                std::fs::create_dir_all(parent)?;
                tracing::trace!("created parent dirs");
            }
        }

        tracing::trace!("repo path: {}", entry.repo_path.display());
        std::fs::copy(&entry.repo_path, &entry.system_path)?;
        tracing::trace!(from=?entry.repo_path, to=?entry.system_path, "copied file");
    }

    Ok(())
}

#[instrument(skip(repo))]
pub fn push(repo: &Repo, override_branch: Option<&str>) -> Result<()> {
    if repo.check_has_unsaved()? {
        print_unsaved_changes_warning();
        return Ok(());
    }

    let branch_name = override_branch.unwrap_or(&STATE.config.upstream.branch);

    repo.push(branch_name)?;

    Ok(())
}

#[instrument]
pub fn verify_local_file_cache() -> Result<()> {
    let mut file_manager = FileManager::new()?;

    let cache_verdict = file_manager.verify_cache()?;

    tracing::trace!("got cache verdict: {cache_verdict:?}");

    match cache_verdict {
        CacheVerdict::FullPopulate => file_manager.write_cache()?,
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
                        file_manager.manage(file.system_path, file.encrypted)?;
                    }
                    "skip" => {
                        tracing::trace!("skipping file");
                    }
                    _ => unreachable!(),
                }
            }

            file_manager.persist_metadata()?;
            file_manager.write_cache()?;
        }
        CacheVerdict::DoNothing => {}
    };

    Ok(())
}
