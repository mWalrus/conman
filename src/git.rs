use std::path::PathBuf;

use anyhow::Result;
use git2::{
    build::{CheckoutBuilder, RepoBuilder},
    AnnotatedCommit, AutotagOption, Cred, CredentialType, Error, FetchOptions, MergeAnalysis,
    PushOptions, Reference, Remote, RemoteCallbacks, Repository,
};

use crate::{file::FileManager, state::STATE};

pub struct Repo {
    inner: Repository,
    refname: String,
}

// TODO:
// 1. we need to inform the user if the specified branch doesn't exist upstream
//    and let them create the branch locally
impl Repo {
    pub fn open() -> Result<Self> {
        let _span = tracing::trace_span!("open").entered();

        let repo_path = &*STATE.paths.repo;

        tracing::trace!(path=?repo_path, "attempting to open repo");
        let repo = Repository::open(&repo_path).unwrap();
        tracing::trace!(path=?repo_path, "opened repo");

        let repo = Self::new_internal(repo);

        if repo.needs_to_update_head()? {
            repo.update_head()?;
        }

        Ok(repo)
    }

    fn new_internal(repo: Repository) -> Self {
        let refname = format!("refs/heads/{}", STATE.config.upstream.branch);
        tracing::trace!("refname set to {refname}");

        Self {
            inner: repo,
            refname,
        }
    }

    fn needs_to_update_head(&self) -> Result<bool> {
        let _span = tracing::trace_span!("needs_to_update_head").entered();

        let head = self.inner.find_reference("HEAD")?;
        let needs_update = head
            .symbolic_target()
            .map(|r| !r.eq(&self.refname))
            .unwrap_or(false);

        tracing::trace!(head=?head.symbolic_target(), ref=self.refname ,"update head?: {needs_update}");

        Ok(needs_update)
    }

    fn update_head(&self) -> Result<()> {
        let _span = tracing::trace_span!("update_head").entered();

        match self.inner.find_reference(&self.refname) {
            Ok(reference) => {
                let name = match reference.name() {
                    Some(name) => name.to_string(),
                    None => String::from_utf8_lossy(reference.name_bytes()).to_string(),
                };

                tracing::trace!(
                    refname = self.refname,
                    resolved_name = name,
                    "found reference with name"
                );

                self.inner.set_head(&name)?;
                tracing::trace!("set head to {name}");
                self.inner
                    .checkout_head(Some(CheckoutBuilder::default().force()))?;
                tracing::trace!("checked out new head");
            }
            Err(_) => {
                let head = self.inner.find_reference("HEAD")?;
                let head_commit = head.peel_to_commit()?;

                tracing::trace!(id=?head_commit.id(), "found head commit");
                self.inner.reference(
                    &self.refname,
                    head_commit.id(),
                    true,
                    &format!(
                        "setting {} to {}",
                        STATE.config.upstream.branch,
                        head_commit.id()
                    ),
                )?;
                tracing::trace!("set ref to point to head commit");
                self.inner.set_head(&self.refname)?;
                tracing::trace!("set head to {}", self.refname);
                self.inner.checkout_head(Some(
                    CheckoutBuilder::default()
                        .allow_conflicts(true)
                        .conflict_style_merge(true)
                        .force(),
                ))?;
                tracing::trace!("checked out new head");
            }
        };

        self.set_upstream(&STATE.config.upstream.branch)?;

        Ok(())
    }

    fn set_upstream(&self, branch_name: &str) -> Result<()> {
        let _span = tracing::trace_span!("set_upstream").entered();

        let mut branch = self
            .inner
            .find_branch(branch_name, git2::BranchType::Local)?;

        if let Err(_) = branch.upstream() {
            branch.set_upstream(Some(branch_name))?;
            tracing::trace!("set upstream for branch '{branch_name}' to 'origin/{branch_name}'");
        }

        Ok(())
    }

    fn make_initial_commit(&self) -> Result<()> {
        let _span = tracing::trace_span!("make_initial_commit").entered();

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

    fn credentials(
        _url: &str,
        username_from_url: Option<&str>,
        _allowed_types: CredentialType,
    ) -> Result<Cred, Error> {
        let span = tracing::trace_span!("remote_callbacks");
        let _enter = span.enter();
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

    pub fn clone() -> Result<()> {
        let _span = tracing::trace_span!("clone").entered();

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

        let repo = Self::new_internal(repo);

        if repo_is_empty {
            repo.make_initial_commit()?;
        }

        // TODO:
        // repo.find_remote_branch()?;

        repo.set_upstream("main")?;

        if repo.needs_to_update_head()? {
            repo.update_head()?;
        }

        Ok(())
    }

    // FIXME: we want to be able to discover the remote's branch name in order to set the upstream
    // fn find_remote_branch(&self) -> Result<()> {
    //     let mut remote = self.inner.find_remote("origin")?;
    //     let connection =
    //         remote.connect_auth(git2::Direction::Fetch, Some(Self::remote_callbacks()), None)?;
    //     let remote_branch = connection.default_branch()?;
    //     let branch = remote_branch.as_str().unwrap();
    //     println!("found remote branch for origin: {branch}");
    //     Ok(())
    // }

    /// Add a file from your local system to be managed by conman
    pub fn add(&self, source: PathBuf, encrypt: bool) -> Result<()> {
        let _span = tracing::trace_span!("add").entered();

        let source_path = std::fs::canonicalize(source)?;

        tracing::trace!(source=?source_path, "canonicalized source path");

        let mut file_manager = FileManager::new()?;

        // we return if the file is already managed since we
        // don't want to do anything in this case
        if file_manager.is_already_managed(&source_path) {
            tracing::trace!("file is already managed, skipping");
            return Ok(());
        }

        let destination_path = STATE.paths.repo_local_file_path(&source_path);

        file_manager.copy(&source_path, &destination_path, encrypt)?;
        tracing::trace!("done adding file");

        Ok(())
    }

    pub fn list(&self) -> Result<()> {
        let _span = tracing::trace_span!("list").entered();
        tracing::trace!("listing managed files");

        let file_manager = FileManager::new()?;
        let metadata = file_manager.metadata();
        for file in metadata.iter() {
            println!("{file}");
        }
        Ok(())
    }

    pub fn remove(&self, path: PathBuf) -> Result<()> {
        let _span = tracing::trace_span!("remove").entered();
        tracing::trace!(to_remove=?path, "removing managed file");
        let mut file_manager = FileManager::new()?;
        file_manager.remove(&path)?;
        Ok(())
    }

    pub fn save(&mut self) -> Result<()> {
        let _span = tracing::trace_span!("save").entered();

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

        // FIXME: construct a more descriptive commit message
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

    fn fetch<'r>(&'r self, refs: &[&str], remote: &'r mut Remote) -> Result<AnnotatedCommit<'r>> {
        let _span = tracing::trace_span!("fetch").entered();

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

    fn merge<'r>(&'r self, remote_branch: &str, fetch_commit: AnnotatedCommit<'r>) -> Result<()> {
        let _span = tracing::trace_span!("merge").entered();

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

    fn fast_forward<'r>(
        &'r self,
        remote_reference: &mut Reference<'r>,
        remote_commit: AnnotatedCommit<'r>,
    ) -> Result<()> {
        let _span = tracing::trace_span!("fast_forward").entered();

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

    fn normal_merge<'r>(
        &'r self,
        head_commit: AnnotatedCommit<'r>,
        fetch_commit: AnnotatedCommit<'r>,
    ) -> Result<()> {
        let _span = tracing::trace_span!("normal merge").entered();

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

    pub fn pull(&self) -> Result<()> {
        let _span = tracing::trace_span!("pull").entered();

        let remote_name = "origin";
        let remote_branch = &STATE.config.upstream.branch;

        let mut remote = self.inner.find_remote(&remote_name)?;

        let fetch_commit = self.fetch(&[remote_branch], &mut remote)?;

        self.merge(remote_branch, fetch_commit)?;

        Ok(())
    }

    pub fn push(&self) -> Result<()> {
        let _span = tracing::trace_span!("push").entered();

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

        remote.push(&[&self.refname], Some(&mut push_options))?;
        tracing::trace!("pushed local changes to origin/{}", self.refname);

        Ok(())
    }
}
