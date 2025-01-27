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
        Command::Status => {}
        Command::Edit { path, dont_save } => {}
        Command::Save => Repo::open().unwrap().save().unwrap(),
        Command::Push => {}
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
    }
}
