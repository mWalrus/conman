use args::{Args, Command};
use clap::Parser;
use git::Repo;

mod args;
mod config;
mod file;
mod git;
mod paths;
mod state;

fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    tracing::trace!(command = ?args.command, "running command");

    match args.command {
        Command::Init => {
            Repo::clone().unwrap();
        }
        Command::Diff { no_color } => {
            let repo = Repo::open().unwrap();
        }
        Command::Status => {
            Repo::open().unwrap().status().unwrap();
        }
        Command::Edit { path, save, apply } => {
            Repo::open().unwrap().edit(path, save, apply).unwrap()
        }
        Command::Save => Repo::open().unwrap().save().unwrap(),
        Command::Push => {
            Repo::open().unwrap().push(None).unwrap();
        }
        Command::Pull => {
            Repo::open().unwrap().pull().unwrap();
        }
        Command::Add { path, encrypt } => {
            Repo::open().unwrap().add(path, encrypt).unwrap();
        }
        Command::Remove { path } => {
            Repo::open().unwrap().remove(path).unwrap();
        }
        Command::List => {
            Repo::open().unwrap().list().unwrap();
        }
        Command::Apply { no_confirm } => {
            Repo::open().unwrap().apply(no_confirm).unwrap();
        }
    }
}
