use std::path::PathBuf;

use clap::{Parser, Subcommand};
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

// FIXME: additional commands
//            - create-config (or something like that)
//
#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum Command {
    #[command(about = "clones the repository from upstream (does nothing if already cloned)")]
    Init,
    #[command(about = "view the current upstream..local diff")]
    Diff,
    #[command(about = "view the status of the local copy of your config")]
    Status,
    #[command(about = "edit a tracked file")]
    Edit {
        #[arg(help = "relative or absolute path to file")]
        path: Option<PathBuf>,
        #[arg(
            short,
            long,
            help = "skip copying any changes made into the conman repo",
            required = false
        )]
        skip_update: bool,
    },
    #[command(about = "save any unsaved changes")] // gather all files + commit
    Save,
    #[command(about = "push saved changes to upstream")]
    Push,
    #[command(about = "pull changes from upstream")]
    Pull,
    #[command(about = "add a file to track")]
    Add {
        #[arg(help = "relative or absolute path to file(s)")]
        files: Vec<PathBuf>,
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
        #[arg(help = "relative or absolute path to file(s)")]
        files: Vec<PathBuf>,
    },
    #[command(about = "apply managed configuration")]
    Apply {
        #[arg(help = "specific file(s) to apply")]
        files: Option<Vec<PathBuf>>,
        #[arg(
            long,
            help = "skip asking for confirmation before applying each file",
            required = false
        )]
        no_confirm: bool,
    },
    Discard {
        #[arg(help = "specific file(s) to discard")]
        files: Option<Vec<PathBuf>>,
        #[arg(
            long,
            help = "skip asking for confirmation before applying each file",
            required = false
        )]
        no_confirm: bool,
    },
    #[command(about = "collect any updates made to managed files on disk")]
    Collect {
        #[arg(help = "specific file(s) to collect")]
        files: Option<Vec<PathBuf>>,
        #[arg(
            long,
            help = "skip asking for confirmation before collecting each file",
            required = false
        )]
        no_confirm: bool,
    },
    #[command(about = "manage branches in conman")]
    Branch {
        #[command(subcommand)]
        branch_op: BranchCommand,
    },
}

#[derive(Subcommand, Debug, PartialEq, Eq)]
pub enum BranchCommand {
    #[command(about = "checkout a branch")]
    Checkout { branch: String },
    #[command(about = "list all available branches")]
    List,
    #[command(about = "delete a branch")]
    Delete { branch: String },
    #[command(about = "show the current branch")]
    Current,
}
