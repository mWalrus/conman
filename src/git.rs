use std::path::PathBuf;

use anyhow::Result;
use colored::Colorize;
use dialoguer::{theme::ColorfulTheme, Confirm};
use git2::{
    build::{CheckoutBuilder, RepoBuilder},
    AnnotatedCommit, AutotagOption, Cred, CredentialType, Error, FetchOptions, MergeAnalysis,
    PushOptions, Reference, Remote, RemoteCallbacks, Repository, Status, StatusOptions, Statuses,
};
use tracing::instrument;

use crate::{file::FileManager, paths::METADATA_FILE_NAME, state::STATE};

pub struct Repo {
    inner: Repository,
}

#[derive(Debug)]
struct StatusEntry {
    status: StatusUpdate,
    old: Option<PathBuf>,
    new: Option<PathBuf>,
}

#[derive(Debug)]
enum StatusUpdate {
    New,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
}

impl std::fmt::Display for StatusUpdate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            StatusUpdate::New => "new".green(),
            StatusUpdate::Modified => "modified".yellow(),
            StatusUpdate::Deleted => "deleted".red(),
            StatusUpdate::Renamed => "renamed".magenta(),
            StatusUpdate::TypeChange => "typechange".blue(),
        };
        write!(f, "{}", s.bold())
    }
}

// TODO:
// 1. we need to inform the user if the specified branch doesn't exist upstream
//    and let them create the branch locally
impl Repo {
    #[instrument]
    pub fn open() -> Result<Self> {
        let repo_path = &*STATE.paths.repo;

        tracing::trace!(path=?repo_path, "attempting to open repo");
        let repo = Repository::open(&repo_path).unwrap();
        tracing::trace!(path=?repo_path, "opened repo");

        let mut repo = Self::new_internal(repo);

        // FIXME: if a user defines a config branch and applies it to their system
        //        and then switches to another config branch and applies that, what
        //        happens to files that were managed by the first config branch but
        //        not the second?
        //        Do we want to remove applied files from the user's system or do
        //        we simply warn when we detect a branch switch that has differences
        //        in managed files?
        let branch = &STATE.config.upstream.branch;
        if !repo.head_matches(branch)? {
            repo.checkout(branch)?;
            repo.set_upstream(branch)?;
        }

        Ok(repo)
    }

    #[inline(always)]
    fn new_internal(repo: Repository) -> Self {
        Self { inner: repo }
    }

    #[instrument(skip(self))]
    fn head_matches(&self, branch_name: &str) -> Result<bool> {
        let refname = format!("refs/heads/{branch_name}");

        let head = self.inner.find_reference("HEAD")?;
        let head_matches = head
            .symbolic_target()
            .map(|r| r.eq(&refname))
            .unwrap_or(false);

        tracing::trace!("head matches: {head_matches}");

        Ok(head_matches)
    }

    #[instrument(skip(self))]
    fn set_upstream(&self, branch_name: &str) -> Result<()> {
        let mut branch = self
            .inner
            .find_branch(branch_name, git2::BranchType::Local)?;

        if let Err(_) = branch.upstream() {
            branch.set_upstream(Some(branch_name))?;
            tracing::trace!("set upstream for branch '{branch_name}' to 'origin/{branch_name}'");
        }

        Ok(())
    }

    #[instrument(skip(self))]
    fn make_initial_commit(&self) -> Result<()> {
        let reference = self.inner.find_reference("HEAD")?;
        let reference = reference.symbolic_target();
        tracing::trace!(ref=?reference, "found reference to HEAD");

        let signature = self.inner.signature()?;
        let oid = self.inner.index()?.write_tree()?;
        let tree = self.inner.find_tree(oid)?;

        self.inner.commit(
            reference,
            &signature,
            &signature,
            "system-chore: initial commit",
            &tree,
            &[],
        )?;

        self.inner.index()?.write()?;

        tracing::trace!("wrote initial commit to disk");
        Ok(())
    }

    #[instrument(skip(_url, username_from_url, _allowed_types))]
    fn credentials(
        _url: &str,
        username_from_url: Option<&str>,
        _allowed_types: CredentialType,
    ) -> Result<Cred, Error> {
        let username = username_from_url.unwrap();

        if let Some(key) = STATE.config.upstream.key_file.as_ref() {
            tracing::trace!(username = username, key = ?key, "built ssh credentials");
            Cred::ssh_key(username, None, key, None)
        } else {
            // no creds?
            tracing::trace!(
                username = username,
                "built username cred since no key file was found"
            );
            Cred::username(username)
        }
    }

    #[instrument]
    pub fn clone() -> Result<()> {
        let repo_path = &*STATE.paths.repo;

        if let Ok(true) = std::fs::exists(&repo_path) {
            tracing::trace!("repo path already exists, skipping clone");
            return Ok(());
        }

        let mut remote_callbacks = RemoteCallbacks::new();
        remote_callbacks.credentials(Self::credentials);

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(remote_callbacks);

        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch_options);

        tracing::trace!("finished building repo options");

        let url = &STATE.config.upstream.url;

        tracing::trace!(url = url, "attempting to clone from upstream");
        let repo = builder.clone(url, &repo_path)?;
        tracing::trace!(url = url, "cloned repo from upstream");

        let repo_is_empty = repo.is_empty()?;

        let mut repo = Self::new_internal(repo);

        if repo_is_empty {
            let branch = crate::config::default_branch();
            repo.make_initial_commit()?;
            repo.checkout(&branch)?;
            repo.set_upstream(&branch)?;
            repo.push(Some(&branch))?;
        }

        repo.checkout(&STATE.config.upstream.branch)?;
        repo.set_upstream(&STATE.config.upstream.branch)?;

        Ok(())
    }

    #[instrument(skip(self))]
    fn checkout(&mut self, branch_name: &str) -> Result<()> {
        if self.head_matches(branch_name)? {
            return Ok(());
        }

        let refname = format!("refs/heads/{branch_name}");

        let refname = match self.inner.find_reference(&refname) {
            Ok(reference) => {
                let name = match reference.name() {
                    Some(name) => name.to_string(),
                    None => String::from_utf8_lossy(reference.name_bytes()).to_string(),
                };

                tracing::trace!(
                    refname = refname,
                    resolved_name = name,
                    "found reference with name"
                );

                name
            }
            Err(_) => {
                let head = self.inner.find_reference("HEAD")?;
                let head_commit = head.peel_to_commit()?;

                tracing::trace!(id=?head_commit.id(), "found head commit");
                self.inner.reference(
                    &refname,
                    head_commit.id(),
                    true,
                    &format!("setting {} to {}", branch_name, head_commit.id()),
                )?;
                tracing::trace!("set ref to point to head commit");
                refname
            }
        };

        self.inner.set_head(&refname)?;
        tracing::trace!("set head to {refname}");

        self.inner.checkout_head(Some(
            CheckoutBuilder::default()
                .allow_conflicts(true)
                .conflict_style_merge(true)
                .force(),
        ))?;

        tracing::trace!("checked out new head");

        Ok(())
    }

    /// Add a file from your local system to be managed by conman
    #[instrument(skip(self))]
    pub fn add(&self, source: PathBuf, encrypt: bool) -> Result<()> {
        let source_path = std::fs::canonicalize(source)?;

        tracing::trace!(source=?source_path, "canonicalized source path");

        let mut file_manager = FileManager::new()?;

        // we return if the file is already managed since we
        // don't want to do anything in this case
        if file_manager.is_already_managed(&source_path) {
            tracing::trace!("file is already managed, skipping");
            return Ok(());
        }

        let destination_path = STATE.paths.repo_local_file_path(&source_path)?;

        file_manager.copy(source_path, destination_path, encrypt)?;
        tracing::trace!("done adding file");

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn list(&self) -> Result<()> {
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

    #[instrument(skip(self))]
    pub fn remove(&self, path: PathBuf) -> Result<()> {
        tracing::trace!(to_remove=?path, "removing managed file");
        let mut file_manager = FileManager::new()?;
        file_manager.remove(&path)?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn save(&mut self) -> Result<()> {
        let mut index = self.inner.index()?;

        // FIXME: fetch status updates here before pushing so that we can construct a more descriptive commit message below
        //        Example:
        //            system-update: updating files
        //
        //            new: /path/to/file/with/config.toml
        //            modified: /path/to/another/file.yml
        //            deleted: /path/to/the/most/epic/config.cfg

        index.add_all(&["."], git2::IndexAddOption::DEFAULT, None)?;
        tracing::trace!("staged all files");

        let oid = index.write_tree()?;
        let signature = self.inner.signature()?;
        let tree = self.inner.find_tree(oid)?;

        let head = self.inner.find_reference("HEAD")?;
        let parent_commit = head.peel_to_commit()?;
        let reference = head.symbolic_target();

        tracing::trace!(ref=?reference, parent=?parent_commit.id(), "preparing commit");

        let commit_oid = self.inner.commit(
            reference,
            &signature,
            &signature,
            "system-update: updating files",
            &tree,
            &[&parent_commit],
        )?;

        index.write()?;
        tracing::trace!(commit=?commit_oid, "wrote commit to disk");

        Ok(())
    }

    #[instrument(skip(self), fields(remote = remote.name()))]
    fn fetch<'r>(&'r self, refs: &[&str], remote: &'r mut Remote) -> Result<AnnotatedCommit<'r>> {
        // FIXME: add callbacks to report progress
        let mut remote_callbacks = RemoteCallbacks::new();
        remote_callbacks.credentials(Self::credentials);

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(remote_callbacks);
        fetch_options.download_tags(AutotagOption::All);

        remote.fetch(refs, Some(&mut fetch_options), None)?;

        tracing::trace!(refs = ?refs, "fetched data from remote");

        let fetch_head = self.inner.find_reference("FETCH_HEAD")?;

        tracing::trace!("got FETCH_HEAD reference");

        let fetch_commit = self.inner.reference_to_annotated_commit(&fetch_head)?;

        tracing::trace!(
            commit = ?fetch_commit.id(),
            "created annotated commit from FETCH_HEAD reference"
        );
        Ok(fetch_commit)
    }

    #[instrument(skip(self), fields(fetch_commit = ?fetch_commit.id()))]
    fn merge<'r>(&'r self, remote_branch: &str, fetch_commit: AnnotatedCommit<'r>) -> Result<()> {
        let analysis = self.inner.merge_analysis(&[&fetch_commit])?;
        match analysis.0 {
            MergeAnalysis::ANALYSIS_FASTFORWARD => {
                tracing::trace!("performing a fast-forward merge");
                let refname = format!("refs/heads/{remote_branch}");
                match self.inner.find_reference(&refname) {
                    Ok(mut reference) => {
                        self.fast_forward(&mut reference, fetch_commit)?;
                    }
                    Err(_) => {
                        let remote_commit_id = fetch_commit.id();
                        self.inner.reference(
                            &refname,
                            remote_commit_id,
                            true,
                            &format!("setting {remote_branch} to {remote_commit_id}"),
                        )?;
                        tracing::trace!(commit_id=?remote_commit_id, "created reference to remote commit");

                        self.inner.set_head(&refname)?;
                        self.inner.checkout_head(Some(
                            CheckoutBuilder::default()
                                .allow_conflicts(true)
                                .conflict_style_merge(true)
                                .force(),
                        ))?;
                        tracing::trace!("set head to {refname}");
                    }
                }
            }
            MergeAnalysis::ANALYSIS_NORMAL => {
                tracing::trace!("performing a normal merge");
                let head = self.inner.head()?;
                let head_commit = self.inner.reference_to_annotated_commit(&head)?;
                self.normal_merge(head_commit, fetch_commit)?;
            }
            MergeAnalysis::ANALYSIS_NONE => {
                // FIXME: warn?
                tracing::trace!("no merge possible");
            }
            MergeAnalysis::ANALYSIS_UP_TO_DATE => {
                tracing::trace!("local branch is up-to-date, nothing to do");
            }
            MergeAnalysis::ANALYSIS_UNBORN => {
                tracing::trace!("HEAD unborn, pointing HEAD to fetch commit");
                if let Some(refname) = fetch_commit.refname() {
                    self.inner.set_head(refname)?;
                }
            }
            _ => {
                unreachable!()
            }
        }
        Ok(())
    }

    #[instrument(skip(self), fields(remote_reference = remote_reference.name(), remote_commit = ?remote_commit.id()))]
    fn fast_forward<'r>(
        &'r self,
        remote_reference: &mut Reference<'r>,
        remote_commit: AnnotatedCommit<'r>,
    ) -> Result<()> {
        let name = match remote_reference.name() {
            Some(name) => name.to_string(),
            None => String::from_utf8_lossy(remote_reference.name_bytes()).to_string(),
        };

        tracing::trace!(name = name, "got remote name");

        let remote_commit_id = remote_commit.id();

        let message = format!("system-fast-forward: setting {name} to {remote_commit_id}");

        remote_reference.set_target(remote_commit_id, &message)?;
        tracing::trace!("set remote to point to {remote_commit_id}");

        self.inner.set_head(&name)?;
        self.inner
            .checkout_head(Some(CheckoutBuilder::default().force()))?;
        tracing::trace!("set head to remote '{name}'");

        Ok(())
    }

    #[instrument(skip(self), fields(head_commit = ?head_commit.id(), fetch_commit = ?fetch_commit.id()))]
    fn normal_merge<'r>(
        &'r self,
        head_commit: AnnotatedCommit<'r>,
        fetch_commit: AnnotatedCommit<'r>,
    ) -> Result<()> {
        let local_id = head_commit.id();
        let remote_id = fetch_commit.id();

        let local_tree = self.inner.find_commit(local_id)?.tree()?;
        let remote_tree = self.inner.find_commit(remote_id)?.tree()?;

        let merge_base = self.inner.merge_base(local_id, remote_id)?;
        tracing::trace!(
            local = ?local_id,
            remote = ?remote_id,
            merge_base = ?merge_base,
            "found merge base"
        );
        let ancestor = self.inner.find_commit(merge_base)?.tree()?;
        let mut index = self
            .inner
            .merge_trees(&ancestor, &local_tree, &remote_tree, None)?;
        tracing::trace!("merged local and remote trees");

        if index.has_conflicts() {
            // FIXME: warn?
            tracing::trace!("merge conflicts detected...");
            self.inner.checkout_index(Some(&mut index), None)?;
            return Ok(());
        }

        let tree_oid = index.write_tree_to(&self.inner)?;
        tracing::trace!("wrote index to repo");
        let result_tree = self.inner.find_tree(tree_oid)?;

        let message = format!("system-merge: {remote_id} into {local_id}");
        let signature = self.inner.signature()?;
        let local_commit = self.inner.find_commit(local_id)?;
        let remote_commit = self.inner.find_commit(remote_id)?;

        let merge_commit = self.inner.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &message,
            &result_tree,
            &[&local_commit, &remote_commit],
        )?;
        tracing::trace!(merge_commit=?merge_commit, "made merge commit");

        self.inner.checkout_head(None)?;
        tracing::trace!("checked out head");
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn pull(&self) -> Result<()> {
        if self.check_has_unsaved()? {
            return Ok(());
        }

        let remote_name = "origin";
        let remote_branch = &STATE.config.upstream.branch;

        let mut remote = self.inner.find_remote(&remote_name)?;

        let fetch_commit = self.fetch(&[remote_branch], &mut remote)?;

        self.merge(remote_branch, fetch_commit)?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn status(&self) -> Result<()> {
        let entries = self.status_entries()?;

        if !self.has_changes(&entries)? {
            println!("{}", "No changes detected, there is nothing to do".green());
            return Ok(());
        }

        let updates = self.prepare_status_updates(entries)?;

        self.print_status_updates(updates);

        Ok(())
    }

    fn print_status_updates(&self, updates: Vec<StatusEntry>) {
        println!("{}", "Unsaved changes:".bold());

        for update in updates.iter() {
            match (update.old.as_ref(), update.new.as_ref()) {
                (Some(old), Some(new)) if old != new => {
                    println!("{}: {} -> {}", update.status, old.display(), new.display());
                }
                (old, new) => {
                    println!("{}: {}", update.status, old.or(new).unwrap().display())
                }
            }
        }
    }

    #[instrument(skip(self))]
    fn status_entries(&self) -> Result<Statuses<'_>> {
        let mut status_options = StatusOptions::new();
        status_options.include_untracked(true);

        let status_entries = self.inner.statuses(Some(&mut status_options))?;
        tracing::trace!("got status entries");

        Ok(status_entries)
    }

    #[instrument(skip(self, entries), fields(has_changes))]
    fn has_changes(&self, entries: &Statuses<'_>) -> Result<bool> {
        let has_changes = entries.iter().any(|entry| {
            entry.status().intersects(
                Status::WT_NEW
                    | Status::WT_MODIFIED
                    | Status::WT_DELETED
                    | Status::WT_RENAMED
                    | Status::WT_TYPECHANGE,
            )
        });

        tracing::Span::current().record("has_changes", has_changes);

        Ok(has_changes)
    }

    #[instrument(skip(self, entries))]
    fn prepare_status_updates(&self, entries: Statuses<'_>) -> Result<Vec<StatusEntry>> {
        let mut status_updates = Vec::with_capacity(entries.len());

        for status_entry in entries.iter() {
            let workdir_tree_status = match status_entry.status() {
                s if s.contains(Status::WT_NEW) => StatusUpdate::New,
                s if s.contains(Status::WT_MODIFIED) => StatusUpdate::Modified,
                s if s.contains(Status::WT_DELETED) => StatusUpdate::Deleted,
                s if s.contains(Status::WT_RENAMED) => StatusUpdate::Renamed,
                s if s.contains(Status::WT_TYPECHANGE) => StatusUpdate::TypeChange,
                _ => continue,
            };

            if status_entry
                .path()
                .map(|path| path.ends_with(METADATA_FILE_NAME))
                .unwrap_or(false)
            {
                tracing::trace!(
                    path = status_entry.path(),
                    "found metadata file, omitting status"
                );
                continue;
            }

            let Some(diff) = status_entry.index_to_workdir() else {
                tracing::trace!("no diff between index and workdir found for the current entry");
                continue;
            };

            let old_path = diff.old_file().path().map(|path| path.to_path_buf());
            let new_path = diff.new_file().path().map(|path| path.to_path_buf());

            status_updates.push(StatusEntry {
                status: workdir_tree_status,
                old: old_path,
                new: new_path,
            });
        }
        Ok(status_updates)
    }

    #[instrument(skip(self))]
    fn check_has_unsaved(&self) -> Result<bool> {
        let status_entries = self.status_entries()?;
        if self.has_changes(&status_entries)? {
            println!(
                "{}",
                "You have unsaved changes, please save them first".yellow()
            );
            let updates = self.prepare_status_updates(status_entries)?;
            self.print_status_updates(updates);
            return Ok(true);
        }
        Ok(false)
    }

    #[instrument(skip(self))]
    pub fn apply(&self, no_confirm: bool) -> Result<()> {
        if self.check_has_unsaved()? {
            return Ok(());
        }

        let file_manager = FileManager::new()?;
        let metadata = file_manager.metadata();

        let zipped = metadata
            .iter()
            .map(|metadata| {
                let file_name = metadata.system_path.file_name().unwrap();
                STATE.paths.repo.join(file_name)
            })
            .zip(metadata.iter());

        for (repo_path, metadata) in zipped {
            tracing::trace!(repo_path=?repo_path, disk_path=?metadata.system_path, encrypted=metadata.encrypted, "handling file");
            if !no_confirm {
                let prompt = format!("Do you want to apply '{}'", metadata.system_path.display());
                if let Ok(false) = Confirm::with_theme(&ColorfulTheme::default())
                    .with_prompt(prompt)
                    .interact()
                {
                    continue;
                }
            }
            std::fs::copy(&repo_path, &metadata.system_path)?;
            tracing::trace!(from=?repo_path, to=?metadata.system_path, "copied file");
        }

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn push(&self, override_branch: Option<&str>) -> Result<()> {
        let branch_name = override_branch.unwrap_or(&STATE.config.upstream.branch);

        if self.check_has_unsaved()? {
            return Ok(());
        }

        let mut remote_callbacks = RemoteCallbacks::new();
        remote_callbacks.push_update_reference(|refname, status| {
            let _span = tracing::trace_span!("push_update_reference").entered();
            match status {
                Some(status) => {
                    tracing::trace!(status = status, "push rejected for ref {refname}");
                }
                None => {
                    tracing::trace!("ref {refname} was updated");
                }
            }
            Ok(())
        });
        remote_callbacks.credentials(Self::credentials);

        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(remote_callbacks);

        let mut remote = self.inner.find_remote("origin")?;
        tracing::trace!("found remote origin");

        let refspec = format!("refs/heads/{branch_name}");
        remote.push(&[refspec], Some(&mut push_options))?;
        tracing::trace!("pushed local changes to origin/{branch_name}",);

        Ok(())
    }

    #[instrument(skip(self, path, skip_update))]
    pub fn edit(&self, path: Option<PathBuf>, skip_update: bool) -> Result<()> {
        FileManager::new()?.edit_managed_file(path, skip_update)?;
        Ok(())
    }

    #[instrument(skip(self, path, no_confirm))]
    pub fn collect(&self, path: Option<PathBuf>, no_confirm: bool) -> Result<()> {
        FileManager::new()?.collect(path, no_confirm)?;
        Ok(())
    }
}
