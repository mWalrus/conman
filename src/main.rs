use args::Args;
use clap::Parser;
use config::Config;

mod args;
mod config;
mod directories;

fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    println!("{args:?}");

    let config = Config::read().unwrap();
    println!("{config:?}");
}
