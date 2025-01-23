use args::{Args, Command};
use clap::Parser;
use git::Repo;

mod args;
mod config;
mod directories;
mod file;
mod git;

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
        Command::Save => {}
        Command::Push => {}
        Command::Pull => {}
        Command::Add { path, encrypt } => {
            let repo = Repo::open().unwrap();
            // TODO: add a discard command to discard unsaved changes

            // NOTES:
            // - when adding a file we will support both relative and absolute paths
            // - the user has to have permission to edit files at said path
            // - a specified path will be copied to the local conman repository
            // - directories are not supported, just add files one by one

            repo.add(path, encrypt).unwrap();
        }
    }
}
