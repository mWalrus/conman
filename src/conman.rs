use std::path::PathBuf;

use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm, FuzzySelect};
use tracing::instrument;

use crate::{
    config::Config,
    file::{self, write_cache, CacheVerdict, FileData, Metadata},
    git::{Repo, StatusType, StatusUpdate},
    paths::Paths,
};

#[instrument(skip(paths, config))]
pub fn init(paths: &Paths, config: &Config) -> Result<()> {
    Repo::clone(paths, config)?;
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

#[instrument(skip(paths, repo))]
pub fn save(paths: &Paths, repo: &Repo) -> Result<()> {
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

    let metadata = Metadata::read(&paths.metadata)?;

    let commit_message = construct_commit_message(&metadata, status_changes);
    repo.commit_changes(commit_message)?;

    Ok(())
}

#[instrument(skip(metadata, status_changes))]
fn construct_commit_message(metadata: &Metadata, status_changes: Vec<StatusUpdate>) -> String {
    let mut commit_message = "system-update: updating files\n\n".to_string();
    let change_count = status_changes.len();

    for (i, entry) in status_changes.into_iter().enumerate() {
        let file_path = entry.old.or(entry.new).unwrap();
        let Some(file_data) = metadata.get_file_data_where_repo_path_ends_with(&file_path) else {
            continue;
        };

        let update = format!(
            "{}: {}{}",
            entry.status.to_str(),
            file_data.system_path.display(),
            if i + 1 == change_count { "" } else { "\n" }
        );

        commit_message.push_str(&update);
    }
    commit_message
}

#[instrument(skip(repo, config))]
pub fn pull(config: &Config, repo: &Repo) -> Result<()> {
    if repo.check_has_unsaved()? {
        print_unsaved_changes_warning();
        return Ok(());
    }

    repo.pull(config)?;

    Ok(())
}

#[instrument(skip(paths, config))]
pub fn edit(
    paths: &Paths,
    config: &Config,
    path: Option<PathBuf>,
    skip_update: bool,
) -> Result<()> {
    let metadata = Metadata::read(&paths.metadata)?;

    let maybe_file_data = match path {
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

    if skip_update {
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

#[instrument(skip(paths, config))]
pub fn collect(
    paths: &Paths,
    config: &Config,
    maybe_specified_path: Option<PathBuf>,
    no_confirm: bool,
) -> Result<()> {
    let mut metadata = Metadata::read(&paths.metadata)?;

    if let Some(specified_path) = maybe_specified_path {
        metadata
            .files
            .retain(|file| file.system_path.eq(&specified_path));
    }

    for file in metadata.files.iter() {
        if !file::source_was_updated(&file.system_path, &file.repo_path)? {
            tracing::trace!("source has not been updated since last time");
            continue;
        }

        if no_confirm {
            file::copy_from_system(file, &config.encryption.passphrase)?;
            continue;
        }

        let message = format!("Collect updated file '{}'?", file.system_path.display());
        let confirmation = dialoguer::Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(message)
            .interact()?;

        tracing::trace!("user gave confirmation: {confirmation}");
        if confirmation {
            file::copy_from_system(file, &config.encryption.passphrase)?;
        }
    }

    Ok(())
}

#[instrument(skip(paths, config))]
pub fn add(paths: &Paths, config: &Config, sources: Vec<PathBuf>, encrypt: bool) -> Result<()> {
    let mut metadata = Metadata::read(&paths.metadata)?;

    for source in sources.into_iter() {
        let source_path = std::fs::canonicalize(source)?;

        tracing::trace!(source=?source_path, "canonicalized source path");

        if metadata.file_is_already_managed(&source_path) {
            tracing::trace!("file is already managed, skipping");
            return Ok(());
        }

        let destination_path = paths.repo_local_file_path(&source_path)?;

        let file_data = FileData::new(source_path, destination_path, encrypt);

        file::copy_from_system(&file_data, &config.encryption.passphrase)?;

        metadata.manage_file(file_data);
    }

    metadata.persist()?;

    file::write_cache(&metadata, &paths.metadata_cache)?;

    Ok(())
}

#[instrument(skip(paths))]
pub fn list(paths: &Paths) -> Result<()> {
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

#[instrument(skip(paths))]
pub fn remove(paths: &Paths, files: Vec<PathBuf>) -> Result<()> {
    let mut metadata = Metadata::read(&paths.metadata)?;

    for file in files {
        let Some(file_data) = metadata.unmanage_file(&file)? else {
            return Ok(());
        };

        file::remove_from_repo(&file_data)?;
    }

    metadata.persist()?;
    Ok(())
}

#[instrument(skip(paths, config, repo))]
pub fn apply(
    paths: &Paths,
    config: &Config,
    repo: &Repo,
    files: Option<Vec<PathBuf>>,
    no_confirm: bool,
) -> Result<()> {
    if repo.check_has_unsaved()? {
        print_unsaved_changes_warning();
        return Ok(());
    }

    let mut metadata = Metadata::read(&paths.metadata)?;

    if let Some(files) = files {
        metadata
            .files
            .retain(|file| files.contains(&file.system_path));
    }

    for file_data in metadata.files.iter() {
        tracing::trace!(entry=?file_data.system_path ,"handling file");
        if !no_confirm {
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

#[instrument(skip(paths, config, repo))]
pub fn discard(paths: &Paths, config: &Config, repo: &Repo, no_confirm: bool) -> Result<()> {
    let mut metadata = Metadata::read(&paths.metadata)?;

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

    let mut confirmed_changes_to_reset = vec![];

    for change in status_changes.into_iter() {
        let repo_path = change.path().unwrap();

        let Some(file_data) = metadata.get_file_data_where_repo_path_ends_with(&repo_path) else {
            continue;
        };

        let change_string = match change.status {
            StatusType::New => "new file",
            _ => "changes made to file",
        };

        let prompt = format!(
            "Do you want to discard {} '{}'",
            change_string,
            file_data.system_path.display()
        );

        let confirmation = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(prompt)
            .interact()?;

        if !confirmation {
            continue;
        }

        confirmed_changes_to_reset.push(change);
    }

    repo.reset(&confirmed_changes_to_reset)?;

    let mut should_persist_metadata = false;

    for change in confirmed_changes_to_reset.iter() {
        let path = change.path().unwrap();

        let Some(file_data) = metadata.get_file_data_where_repo_path_ends_with(path) else {
            continue;
        };

        match change.status {
            StatusType::New => {
                // EXPLANATION: We have to clone because borrow checker
                let system_path = file_data.system_path.clone();
                metadata.unmanage_file(&system_path)?;
                should_persist_metadata = true;
            }
            StatusType::Modified => {
                file::copy_from_repo(file_data, &config.encryption.passphrase)?;
            }
            StatusType::Deleted => {
                metadata.manage_file(file_data.clone());
                should_persist_metadata = true;
            }
            _ => {}
        }
    }

    if should_persist_metadata {
        metadata.persist()?;
        write_cache(&metadata, &paths.metadata_cache)?;
    }

    Ok(())
}

#[instrument(skip(config, repo))]
pub fn push(config: &Config, repo: &Repo, upstream_branch: &str) -> Result<()> {
    if repo.check_has_unsaved()? {
        print_unsaved_changes_warning();
        return Ok(());
    }

    repo.push(config, &config.upstream.branch)?;

    Ok(())
}

#[instrument(skip(paths))]
pub fn verify_local_file_cache(paths: &Paths, config: &Config) -> Result<()> {
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

#[instrument(skip(config, repo))]
pub fn branch(config: &mut Config, repo: &Repo, branch_name: &str, delete: bool) -> Result<()> {
    if delete {
        let confirmation = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!("Are you sure you want to delete {branch_name}"))
            .interact()?;

        if confirmation {
            repo.delete_branch(branch_name)?;
        }

        return Ok(());
    }

    config.upstream.branch = branch_name.to_string();

    repo.checkout(&config.upstream.branch)?;
    repo.set_upstream(&config.upstream.branch)?;

    config.write()?;
    Ok(())
}
