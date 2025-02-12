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

    Operation::verify_cache().unwrap().execute_blocking();

    tracing::trace!(command = ?args.command, "running command");

    let mut operation = Operation::new(args.command).unwrap();

    let receiver = operation.subscribe();

    operation.execute();

    while let Ok(message) = receiver.recv() {
        println!("message: {message}");
    }
}
