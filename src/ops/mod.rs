use std::{fmt::Display, thread::JoinHandle};

use add::AddOp;
use anyhow::Result;
use apply::ApplyOp;
use clone::CloneOp;
use collect::CollectOp;
use crossbeam_channel::{Receiver, Sender};
use diff::DiffOp;
use discard::DiscardOp;
use edit::EditOp;
use list::ListOp;
use push::PushOp;
use remove::RemoveOp;
use save::SaveOp;
use status::StatusOp;
use verify_cache::VerifyCacheOp;

use crate::{
    args::{BranchCommand, Command},
    config::Config,
    paths::Paths,
};

pub mod add;
pub mod apply;
pub mod branch;
pub mod clone;
pub mod collect;
pub mod diff;
pub mod discard;
pub mod edit;
pub mod list;
pub mod pull;
pub mod push;
pub mod remove;
pub mod save;
pub mod status;
pub mod verify_cache;

#[macro_export]
macro_rules! report {
    ($sender:tt, $message:expr) => {
        if let Some(tx) = $sender.as_ref() {
            match tx.send(Box::new($message)) {
                Ok(()) => {},
                Err(e) => eprintln!("ERROR: {e:?}"),
            }
        }
    };
    ($sender:tt, $base:expr, $($arg:expr),*) => {
        report!($sender, format!($base, $($arg),*))
    };
}

pub trait Runnable {
    fn run(
        &self,
        config: Config,
        paths: Paths,
        sender: Option<Sender<Box<dyn Display + Send + Sync>>>,
    ) -> Result<()>;

    fn run_silent(&self, config: Config, paths: Paths) -> Result<()> {
        self.run(config, paths, None)
    }
}

type RunnableOperation = Box<dyn Runnable + Send + Sync>;

pub struct Operation {
    tx: Option<Sender<Box<dyn Display + Send + Sync>>>,
    inner: RunnableOperation,
    config: Config,
    paths: Paths,
}

impl Operation {
    pub fn new(command: Command) -> Result<Self> {
        let inner: RunnableOperation = match command {
            Command::Init => Box::new(CloneOp),
            Command::Branch { branch_op } => match branch_op {
                BranchCommand::Checkout { branch } => Box::new(branch::CheckoutOp(branch)),
                BranchCommand::List => Box::new(branch::ListOp),
                BranchCommand::Delete { branch } => Box::new(branch::DeleteOp(branch)),
                BranchCommand::Current => Box::new(branch::CurrentOp),
            },
            Command::Diff => Box::new(DiffOp),
            Command::Status => Box::new(StatusOp),
            Command::Edit { path, skip_update } => Box::new(EditOp { path, skip_update }),
            Command::Save => Box::new(SaveOp),
            Command::Push => Box::new(PushOp),
            Command::Pull => todo!(),
            Command::Add { files, encrypt } => Box::new(AddOp { files, encrypt }),
            Command::List => Box::new(ListOp),
            Command::Remove { files } => Box::new(RemoveOp { files }),
            Command::Apply { files, no_confirm } => Box::new(ApplyOp { files, no_confirm }),
            Command::Discard { files, no_confirm } => Box::new(DiscardOp { files, no_confirm }),
            Command::Collect { files, no_confirm } => Box::new(CollectOp { files, no_confirm }),
        };

        let config = Config::read()?;
        let paths = Paths::new()?;

        Ok(Self {
            tx: None,
            inner,
            config,
            paths,
        })
    }

    pub fn verify_cache() -> Result<Self> {
        let config = Config::read()?;
        let paths = Paths::new()?;

        Ok(Self {
            tx: None,
            inner: Box::new(VerifyCacheOp),
            config,
            paths,
        })
    }

    pub fn subscribe(&mut self) -> Receiver<Box<dyn Display + Send + Sync>> {
        let (tx, rx) = crossbeam_channel::unbounded();
        self.tx = Some(tx);
        rx
    }

    /// execute the operation in a separate thread
    pub fn execute(self) -> JoinHandle<Result<()>> {
        std::thread::spawn(move || self.inner.run(self.config, self.paths, self.tx))
    }

    /// execute the operation, blocking the main thread
    pub fn execute_blocking(self) -> Result<()> {
        self.inner.run_silent(self.config, self.paths)
    }
}
