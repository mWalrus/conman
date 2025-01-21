use data::config::Config;

mod data;

fn main() {
    tracing_subscriber::fmt::init();
    let config = Config::read().unwrap();
}
