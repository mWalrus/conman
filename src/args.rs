use std::path::PathBuf;

use clap::{Parser, Subcommand};
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    #[command(about = "clones the repository from upstream (does nothing if already cloned)")]
    Init,
    #[command(about = "view the current upstream..local diff")]
    Diff {
        #[arg(short, long)]
        no_color: bool,
    },
    #[command(about = "view the status of the local copy of your config")]
    Status,
    #[command(about = "edit a tracked file")]
    Edit {
        #[arg(help = "relative or absolute path to file")]
        path: PathBuf,
        #[arg(short, long, help = "don't save on exit", required = false)]
        dont_save: bool,
    },
    #[command(about = "save any unsaved changes")] // gather all files + commit
    Save,
    #[command(about = "push saved changes to upstream")]
    Push,
    #[command(about = "pull changes from upstream")]
    Pull,
    #[command(about = "add a file to track")]
    Add {
        #[arg(help = "relative or absolute path to file")]
        path: PathBuf,
        #[arg(
            short,
            long,
            help = "flags this file as to be encrypted",
            required = false
        )]
        encrypt: bool,
    },
    #[command(about = "list all managed files")]
    List,
    #[command(about = "remove a managed file")]
    Remove {
        #[arg(help = "relative or absolute path to file")]
        path: PathBuf,
    },
}
