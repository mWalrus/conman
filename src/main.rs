use args::{Args, Command};
use clap::Parser;
use git::Repo;
use state::STATE;

mod args;
mod cache;
mod config;
mod conman;
mod file;
mod git;
mod paths;
mod state;

fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    tracing::trace!(command = ?args.command, "running command");

    if args.command == Command::Init {
        conman::init().unwrap();
        return;
    }

    let mut repo = Repo::open().unwrap();

    // NOTE: we have to checkout here if we want to reliably detect
    //       cache changes
    if !repo.head_matches(&STATE.config.upstream.branch).unwrap() {
        repo.checkout(&STATE.config.upstream.branch).unwrap();
        repo.set_upstream(&STATE.config.upstream.branch).unwrap();
    }

    conman::verify_local_file_cache().unwrap();

    let result = match args.command {
        Command::Diff { no_color } => conman::diff(&repo, no_color),
        Command::Status => conman::status(&repo),
        Command::Edit { path, skip_update } => conman::edit(path, skip_update),
        Command::Save => conman::save(&repo),
        Command::Push => conman::push(&repo, None),
        Command::Pull => conman::pull(&repo),
        Command::Add { path, encrypt } => conman::add(path, encrypt),
        Command::Remove { path } => conman::remove(path),
        Command::List => conman::list(),
        Command::Apply { no_confirm } => conman::apply(&repo, no_confirm),
        Command::Collect { path, no_confirm } => conman::collect(path, no_confirm),
        Command::Init => unreachable!("we handled this above"),
    };

    if let Err(e) = result {
        eprintln!("{e:?}");
    }
}
