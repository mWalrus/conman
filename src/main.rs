use args::{Args, Command};
use clap::Parser;

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

    conman::verify_local_file_cache().unwrap();

    let result = match args.command {
        Command::Init => conman::init(),
        Command::Diff { no_color } => conman::diff(no_color),
        Command::Status => conman::status(),
        Command::Edit { path, skip_update } => conman::edit(path, skip_update),
        Command::Save => conman::save(),
        Command::Push => conman::push(None),
        Command::Pull => conman::pull(),
        Command::Add { path, encrypt } => conman::add(path, encrypt),
        Command::Remove { path } => conman::remove(path),
        Command::List => conman::list(),
        Command::Apply { no_confirm } => conman::apply(no_confirm),
        Command::Collect { path, no_confirm } => conman::collect(path, no_confirm),
    };

    if let Err(e) = result {
        eprintln!("{e:?}");
    }
}
