use args::{Args, Command};
use clap::Parser;
use config::Config;
use git::Repo;
use paths::Paths;

mod args;
mod config;
mod conman;
mod file;
mod git;
mod paths;

fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    tracing::trace!(command = ?args.command, "running command");

    let mut config = match Config::read() {
        Ok(Some(config)) => config,
        Ok(None) => {
            Config::write_default_config().unwrap();
            return;
        }
        Err(e) => {
            eprintln!("Failed to read config: {e:?}");
            return;
        }
    };

    let paths = match Paths::new(&config) {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Failed to construct file paths: {e:?}");
            return;
        }
    };

    if args.command == Command::Init {
        conman::init(&paths, &config).unwrap();
        return;
    }

    let repo = Repo::open(&paths).unwrap();

    // NOTE: we have to checkout here if we want to reliably detect
    //       cache changes
    if !repo.head_matches(&config.upstream.branch).unwrap() {
        repo.checkout(&config.upstream.branch).unwrap();
        repo.set_upstream(&config.upstream.branch).unwrap();
    }

    conman::verify_local_file_cache(&paths, &config).unwrap();

    let result = match args.command {
        Command::Diff { no_color } => conman::diff(&repo, no_color),
        Command::Status => conman::status(&paths, &repo),
        Command::Edit { path, skip_update } => conman::edit(&paths, &config, path, skip_update),
        Command::Save => conman::save(&paths, &repo),
        Command::Push => conman::push(&config, &repo, &config.upstream.branch),
        Command::Pull => conman::pull(&config, &repo),
        Command::Add { files, encrypt } => conman::add(&paths, &config, files, encrypt),
        Command::Remove { files } => conman::remove(&paths, files),
        Command::List => conman::list(&paths),
        Command::Apply { files, no_confirm } => {
            conman::apply(&paths, &config, &repo, files, no_confirm)
        }
        Command::Discard { files, no_confirm } => {
            conman::discard(&paths, &config, &repo, files, no_confirm)
        }
        Command::Collect { files, no_confirm } => {
            conman::collect(&paths, &config, files, no_confirm)
        }
        Command::Branch {
            checkout,
            list,
            delete,
        } => {
            let mut result: anyhow::Result<(), anyhow::Error> = Ok(());

            if let Some(branch) = checkout {
                result = conman::checkout_branch(&mut config, &repo, branch);
            }

            if list {
                result = conman::list_branches(&repo);
            }

            if let Some(branch) = delete {
                result = conman::delete_branch(&repo, branch);
            }

            result
        }
        Command::Init => unreachable!("we handled this above"),
    };

    if let Err(e) = result {
        eprintln!("{e:?}");
    }
}
