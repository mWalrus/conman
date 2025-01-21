use data::config::Config;

mod config;
mod directories;

fn main() {
    tracing_subscriber::fmt::init();
    let config = Config::read().unwrap();
    println!("{config:?}");
}
