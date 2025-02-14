use std::path::PathBuf;

use anyhow::{anyhow, Result};
use colored::{ColoredString, Colorize};
use git2::{
    build::{CheckoutBuilder, RepoBuilder},
    AnnotatedCommit, AutotagOption, BranchType, Cred, CredentialType, Error, FetchOptions,
    MergeAnalysis, PushOptions, Reference, Remote, RemoteCallbacks, Repository, Status,
    StatusEntry, StatusOptions, Statuses,
};
use tracing::instrument;

use crate::{
    config::Config,
    paths::{Paths, METADATA_FILE_NAME},
};

pub struct Repo {
    inner: Repository,
}

#[derive(Debug)]
pub struct StatusChange {
    pub status: StatusType,
    pub relative_path: PathBuf,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StatusType {
    New,
    Modified,
    Deleted,
    Renamed,
    TypeChange,
}

impl StatusType {
    pub fn into_colored_string(&self) -> ColoredString {
        match self {
            StatusType::New => "new".green(),
            StatusType::Modified => "modified".yellow(),
            StatusType::Deleted => "deleted".red(),
            StatusType::Renamed => "renamed".magenta(),
            StatusType::TypeChange => "typechange".blue(),
        }
    }

    pub fn to_str(&self) -> &'_ str {
        match self {
            StatusType::New => "new",
            StatusType::Modified => "modified",
            StatusType::Deleted => "deleted",
            StatusType::Renamed => "renamed",
            StatusType::TypeChange => "typechange",
        }
    }
}

impl Repo {
    #[instrument(skip(paths))]
    pub fn open(paths: &Paths) -> Result<Self> {
        tracing::trace!(path=?paths.repo, "attempting to open repo");
        let repo = Repository::open(&paths.repo)
            .map_err(|_| anyhow!("repo not found. Use `conman init` to clone it"))?;
        tracing::trace!(path=?paths.repo, "opened repo");

        Ok(Self { inner: repo })
    }

    #[instrument(skip(self))]
    pub fn head_matches(&self, branch_name: &str) -> Result<bool> {
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
    pub fn delete_branch(&self, branch_name: &str) -> Result<()> {
        let mut branch = self.inner.find_branch(branch_name, BranchType::Local)?;

        branch.delete()?;
        tracing::trace!("deleted branch");

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn set_upstream(&self, branch_name: &str) -> Result<()> {
        let mut branch = self.inner.find_branch(branch_name, BranchType::Local)?;

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
        key_file: Option<&PathBuf>,
    ) -> Result<Cred, Error> {
        let username = username_from_url.unwrap();

        if let Some(key) = key_file {
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

    #[instrument(skip(paths))]
    pub fn clone(paths: &Paths, config: &Config) -> Result<()> {
        if let Ok(true) = std::fs::exists(&paths.repo) {
            tracing::trace!("repo path already exists, skipping clone");
            return Ok(());
        }

        let mut remote_callbacks = RemoteCallbacks::new();
        remote_callbacks.credentials(|url, username_from_url, allowed_types| {
            Self::credentials(
                url,
                username_from_url,
                allowed_types,
                config.upstream.key_file.as_ref(),
            )
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(remote_callbacks);

        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch_options);

        tracing::trace!("finished building repo options");

        let url = &config.upstream.url;

        tracing::trace!(url = url, "attempting to clone from upstream");
        let repo = builder.clone(url, &paths.repo)?;
        tracing::trace!(url = url, "cloned repo from upstream");

        let repo_is_empty = repo.is_empty()?;

        let repo = Self { inner: repo };

        if repo_is_empty {
            let branch = crate::config::default_branch();
            repo.make_initial_commit()?;
            repo.checkout(&branch)?;
            repo.set_upstream(&branch)?;
            repo.push(config, &branch)?;
        }

        repo.checkout(&config.upstream.branch)?;
        repo.set_upstream(&config.upstream.branch)?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn checkout(&self, branch_name: &str) -> Result<()> {
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

    #[instrument(skip(self))]
    pub fn commit_changes(&self, commit_message: String) -> Result<()> {
        let mut index = self.inner.index()?;

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
            &commit_message,
            &tree,
            &[&parent_commit],
        )?;

        index.write()?;
        tracing::trace!(commit=?commit_oid, "wrote commit to disk");

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

    #[instrument(skip(self, config), fields(remote = remote.name()))]
    fn fetch<'r>(&'r self, config: &Config, remote: &'r mut Remote) -> Result<AnnotatedCommit<'r>> {
        // FIXME: add callbacks to report progress
        let mut remote_callbacks = RemoteCallbacks::new();
        remote_callbacks.credentials(|url, username_from_url, allowed_types| {
            Self::credentials(
                url,
                username_from_url,
                allowed_types,
                config.upstream.key_file.as_ref(),
            )
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(remote_callbacks);
        fetch_options.download_tags(AutotagOption::All);

        let refs = &[&config.upstream.branch];

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

    #[instrument(skip(self, config))]
    pub fn pull(&self, config: &Config) -> Result<()> {
        let remote_name = "origin";
        let mut remote = self.inner.find_remote(&remote_name)?;

        let fetch_commit = self.fetch(config, &mut remote)?;

        self.merge(&config.upstream.branch, fetch_commit)?;

        Ok(())
    }

    #[instrument(skip(self))]
    pub fn status_changes(&self) -> Result<Option<Vec<StatusChange>>> {
        let status_entries = self.status_entries()?;

        let status_changes: Vec<_> = status_entries
            .iter()
            .filter(|entry| {
                entry.status().intersects(
                    Status::WT_NEW
                        | Status::WT_MODIFIED
                        | Status::WT_DELETED
                        | Status::WT_RENAMED
                        | Status::WT_TYPECHANGE,
                )
            })
            .collect();

        if status_changes.is_empty() {
            tracing::trace!("no status changes found");
            return Ok(None);
        }

        let formatted_changes = self.prepare_status_changes(status_changes)?;

        Ok(Some(formatted_changes))
    }

    #[instrument(skip(self))]
    fn status_entries(&self) -> Result<Statuses<'_>> {
        let mut status_options = StatusOptions::new();
        status_options.include_untracked(true);

        let status_entries = self.inner.statuses(Some(&mut status_options))?;
        tracing::trace!("got status entries");

        Ok(status_entries)
    }

    #[instrument(skip(self, entries))]
    fn prepare_status_changes(&self, entries: Vec<StatusEntry<'_>>) -> Result<Vec<StatusChange>> {
        let mut status_changes = Vec::with_capacity(entries.len());

        for status_entry in entries.iter() {
            let workdir_tree_status = match status_entry.status() {
                s if s.contains(Status::WT_NEW) => StatusType::New,
                s if s.contains(Status::WT_MODIFIED) => StatusType::Modified,
                s if s.contains(Status::WT_DELETED) => StatusType::Deleted,
                s if s.contains(Status::WT_RENAMED) => StatusType::Renamed,
                s if s.contains(Status::WT_TYPECHANGE) => StatusType::TypeChange,
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

            let relative_path = old_path.or(new_path).unwrap();

            status_changes.push(StatusChange {
                status: workdir_tree_status,
                relative_path,
            });
        }
        Ok(status_changes)
    }

    #[instrument(skip(self))]
    pub fn check_has_unsaved(&self) -> Result<bool> {
        self.status_changes().map(|changes| changes.is_some())
    }

    #[instrument(skip(self, config))]
    pub fn push(&self, config: &Config, branch_name: &str) -> Result<()> {
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
        remote_callbacks.credentials(|url, username_from_url, allowed_types| {
            Self::credentials(
                url,
                username_from_url,
                allowed_types,
                config.upstream.key_file.as_ref(),
            )
        });

        let mut push_options = PushOptions::new();
        push_options.remote_callbacks(remote_callbacks);

        let mut remote = self.inner.find_remote("origin")?;
        tracing::trace!("found remote origin");

        let refspec = format!("refs/heads/{branch_name}");
        remote.push(&[refspec], Some(&mut push_options))?;
        tracing::trace!("pushed local changes to origin/{branch_name}",);

        Ok(())
    }

    #[instrument(skip(self, changes_to_reset))]
    pub fn reset(&self, changes_to_reset: &Vec<StatusChange>) -> Result<()> {
        let mut checkout_opts = CheckoutBuilder::new();

        checkout_opts.force();
        checkout_opts.remove_untracked(true);

        for change in changes_to_reset.iter() {
            checkout_opts.path(&change.relative_path);
        }

        self.inner.checkout_head(Some(&mut checkout_opts))?;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn local_branch_names(&self) -> Result<Vec<String>> {
        let branches = self.inner.branches(Some(BranchType::Local))?;

        let branch_names: Vec<_> = branches
            .flatten()
            .map(|branch| branch.0)
            .filter_map(|branch| branch.name().ok()?.map(|name| name.to_string()))
            .collect();

        Ok(branch_names)
    }
}
