use std::sync::LazyLock;

use anyhow::Result;
use tracing::instrument;

use crate::{config::Config, paths::Paths};

// NOTE: static is required for the internal state of the `LazyLock` to live for the
//       entire runtime of the application
pub static STATE: LazyLock<State> = LazyLock::new(|| State::new().unwrap());

pub struct State {
    pub paths: Paths,
    pub config: Config,
}

impl State {
    #[instrument]
    pub fn new() -> Result<Self> {
        let config = Config::read()?;
        let paths = Paths::new(&config)?;
        Ok(Self { paths, config })
    }
}
