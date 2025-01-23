use std::path::PathBuf;

use anyhow::Result;
use git2::{
    build::{CheckoutBuilder, RepoBuilder},
    Cred, FetchOptions, RemoteCallbacks, Repository,
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
        let repo_path = &*STATE.paths.repo;

        tracing::trace!(path=?repo_path, "attempting to open repo");
        let repo = Repository::open(&repo_path).unwrap();
        tracing::trace!(path=?repo_path, "opened repo");

        let refname = format!("refs/heads/{}", STATE.config.upstream.branch);

        let repo = Self {
            inner: repo,
            refname,
        };

        repo.update_head()?;

        Ok(repo)
    }

    fn update_head(&self) -> Result<()> {
        let span = tracing::trace_span!("update_head");
        let _enter = span.enter();

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
                let head = self.inner.head()?;
                let head_commit = self.inner.reference_to_annotated_commit(&head)?;
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

        let branch_name = &STATE.config.upstream.branch;
        let mut branch = self
            .inner
            .find_branch(branch_name, git2::BranchType::Local)?;

        if let Err(_) = branch.upstream() {
            branch.set_upstream(Some(branch_name))?;
            tracing::trace!("set upstream for branch '{branch_name}' to 'origin/{branch_name}'");
        }

        Ok(())
    }

    fn make_initial_commit(repo: &Repository) -> Result<()> {
        let reference = repo.find_reference("HEAD")?;
        let reference = reference.symbolic_target();
        tracing::trace!(ref=?reference, "found reference to HEAD");

        let signature = repo.signature()?;
        let oid = repo.index()?.write_tree()?;
        let tree = repo.find_tree(oid)?;

        repo.commit(
            reference,
            &signature,
            &signature,
            "system-chore: initial commit",
            &tree,
            &[],
        )?;
        repo.index()?.write()?;
        Ok(())
    }

    pub fn clone() -> Result<()> {
        // do nothing if we can successfully open the repo on disk since we
        // don't have to clone if that's the case
        let repo_path = &*STATE.paths.repo;

        if let Ok(true) = std::fs::exists(&repo_path) {
            tracing::trace!("repo path already exists");
            return Ok(());
        }

        let mut callbacks = RemoteCallbacks::new();
        callbacks.credentials(|_url, username_from_url, _allowed_types| {
            tracing::trace!("fetching credentials to use for clone from upstream");
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
        });

        let mut fetch_options = FetchOptions::new();
        fetch_options.remote_callbacks(callbacks);

        let mut builder = RepoBuilder::new();
        builder.fetch_options(fetch_options);

        let url = &STATE.config.upstream.url;

        tracing::trace!(url = url, "attempting to clone from upstream");
        let repo = builder.clone(url, &repo_path)?;
        tracing::trace!(url = url, "cloned repo from upstream");

        if repo.is_empty()? {
            Self::make_initial_commit(&repo)?;
        }

        Ok(())
    }

    /// Add a file from your local system to be managed by conman
    pub fn add(&self, source: PathBuf, encrypt: bool) -> Result<()> {
        let source_path = std::fs::canonicalize(source)?;

        let mut file_manager = FileManager::new()?;

        // we return if the file is already managed since we
        // don't want to do anything in this case
        if file_manager.is_already_managed(&source_path) {
            return Ok(());
        }

        let destination_path = STATE.paths.repo_local_file_path(&source_path);

        file_manager.copy(&source_path, &destination_path, encrypt)?;

        Ok(())
    }

    pub fn list(&self) -> Result<()> {
        let file_manager = FileManager::new()?;
        let metadata = file_manager.metadata();
        for file in metadata.iter() {
            println!("{file}");
        }
        Ok(())
    }

    pub fn remove(&self, path: PathBuf) -> Result<()> {
        let mut file_manager = FileManager::new()?;
        file_manager.remove(&path)?;
        Ok(())
    }

    pub fn save(&mut self) -> Result<()> {
        // FIXME: implement branch support
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

        let parent_commit = self.inner.head()?.peel_to_commit()?;

        let head_reference = self.inner.find_reference("HEAD")?;
        let reference = head_reference.symbolic_target();

        tracing::trace!(tree=?tree, "preparing commit");

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
