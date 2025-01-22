use args::{Args, Command};
use clap::Parser;
use config::Config;
use git::Repo;

mod args;
mod config;
mod directories;
mod git;

fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let config = Config::read().unwrap();

    match args.command {
        Command::Init => {
            tracing::trace!(command = "init", "running command");
            Repo::clone(&config).unwrap();
        }
        Command::Diff { no_color } => {
            tracing::trace!(command = "diff", "running command");
            let repo = Repo::open(&config).unwrap();
        }
        Command::Status => {
            tracing::trace!(command = "status", "running command");
        }
        Command::Edit { path, dont_save } => {
            tracing::trace!(command = "edit", "running command");
        }
        Command::Save => {
            tracing::trace!(command = "save", "running command");
        }
        Command::Push => {
            tracing::trace!(command = "push", "running command");
        }
        Command::Pull => {
            tracing::trace!(command = "pull", "running command");
        }
        Command::Add { path, encrypt } => {
            tracing::trace!(command = "add", "running command");
        }
    }
}
