use std::path::PathBuf;

use anyhow::Result;
use git2::{
    build::{CheckoutBuilder, RepoBuilder},
    Cred, CredentialType, Error, FetchOptions, RemoteCallbacks, Repository,
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
        let head = self.inner.find_reference("HEAD")?;
        let needs_update = head
            .symbolic_target()
            .map(|r| r.eq(&self.refname))
            .unwrap_or(false);
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

        // let branch_name = &STATE.config.upstream.branch;
        // let branch = match self.0.find_branch(branch_name, BranchType::Local) {
        //     Ok(branch) => {
        //         tracing::trace!(branch_name = branch_name, "found branch");
        //         branch
        //     }
        //     Err(_) => {
        //         tracing::trace!(branch_name = branch_name, "no branch with name found");
        //         let latest_commit = self.0.head()?.peel_to_commit()?;
        //         tracing::trace!("creating branch");
        //         self.0.branch(branch_name, &latest_commit, false)?
        //     }
        // };

        // let reference = branch.get();

        // let parent_commit = reference.peel_to_commit()?;
        // tracing::trace!(parent_commit=?parent_commit.id(), "found parent commit");

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
}
