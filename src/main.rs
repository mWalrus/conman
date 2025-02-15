use args::Args;
use clap::Parser;
use ops::Operation;

mod args;
mod config;
mod file;
mod git;
mod ops;
mod paths;

// FIXME: If you decide to swap branches in the config it should be to an already
//        existing branch. We should fail if the branch specified doesn't exist.
//        this forces the user to checkout a new branch by using the `branch checkout`
//        command. This allows us to better check and invalidate cache by writing it
//        only on `save` and check it only on branch switches.
//        One issue though is that if we manually through the config checkout another
//        existing branch we might have to either skip validating cache or figure something
//        else out.
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
