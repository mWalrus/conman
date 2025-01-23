use args::{Args, Command};
use clap::Parser;
use config::Config;
use git::Repo;

mod args;
mod config;
mod directories;
mod file;
mod git;

fn main() {
    tracing_subscriber::fmt::init();

    let config = Config::read().unwrap();

    let args = Args::parse();

    tracing::trace!(command = ?args.command, "running command");

    match args.command {
        Command::Init => {
            Repo::clone(&config).unwrap();
        }
        Command::Diff { no_color } => {
            let repo = Repo::open(&config).unwrap();
        }
        Command::Status => {}
        Command::Edit { path, dont_save } => {}
        Command::Save => {}
        Command::Push => {}
        Command::Pull => {}
        Command::Add { path, encrypt } => {
            Repo::open(&config)
                .unwrap()
                .add(&config, path, encrypt)
                .unwrap();
        }
        Command::Remove { path } => {
            Repo::open(&config).unwrap().remove(&config, path).unwrap();
        }
        Command::List => {
            let repo = Repo::open(&config).unwrap();
            repo.list(&config).unwrap();
        }
    }
}
