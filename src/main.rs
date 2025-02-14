use args::Args;
use clap::Parser;
use ops::Operation;

mod args;
mod config;
mod file;
mod git;
mod ops;
mod paths;

fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    paths::create_dirs().unwrap();

    if let Err(err) = Operation::verify_cache().unwrap().execute_blocking() {
        eprintln!("ERROR: {err:?}");
        return;
    }

    tracing::trace!(command = ?args.command, "running command");

    let mut operation = Operation::new(args.command).unwrap();

    let receiver = operation.subscribe();

    let task_handle = operation.execute();

    while let Ok(message) = receiver.recv() {
        println!("{message}");
    }

    if let Ok(Err(err)) = task_handle.join() {
        eprintln!("ERROR: {err:?}");
    }
}
