use std::{fs, path::PathBuf, sync::LazyLock};

use directories::BaseDirs;

pub(crate) const APPLICATION_NAME: &str = "conman";
pub const DIRECTORIES: LazyLock<Directories> = LazyLock::new(|| Directories::new());

pub struct Directories {
    cache: PathBuf,
    config: PathBuf,
}

impl Directories {
    fn new() -> Self {
        // NOTE: if either of the below fallible operations fail, something unrelated to conman
        //       is wrong and we have to panic

        // SEE: https://docs.rs/directories/latest/directories/struct.BaseDirs.html#method.new
        let base_dirs = BaseDirs::new().unwrap();

        let cache = base_dirs.data_dir().join(APPLICATION_NAME);
        if !fs::exists(&cache).unwrap() {
            fs::create_dir(&cache).unwrap();
            tracing::trace!("created $HOME/.local/share/{APPLICATION_NAME}");
        }

        let config = base_dirs.config_dir().join(APPLICATION_NAME);
        if !fs::exists(&config).unwrap() {
            fs::create_dir(&config).unwrap();
            tracing::trace!("created $HOME/.config/{APPLICATION_NAME}");
        }

        Self { cache, config }
    }
}
