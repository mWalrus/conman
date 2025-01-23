# Project structure

## Data
All data structures related to application data will live in this module.
Things like the local clone of the git repo will be stored here as well as the user defined
configuration of the `conman` itself.

### Storage structure

#### Cache
Cache will be stored in `$HOME/.local/share/conman` and consist of a local clone of the git repo

#### Configuration
Configuration will be stored as any other in `$HOME/.config/conman`.


Example configuration:
```toml
[encryption]
# the user is responsible for this key, both when it comes to strength and not losing it
passphrase = "your_strong_passphrase_123"

# define an ssh-based upstream
[upstream]
url = "git@example.com:user/dotfiles"
# keys are optional
# relative key paths default to '$HOME/.ssh'.
# absolute key paths are also supported.
key_file = "private_key"
branch = "laptop" # optional, will default to main
```


### Files:
- `src/data/mod.rs`: general definitions of paths for managing application data
- `src/data/cache.rs`: interact with locally stored data such as the underlying git repo
- `src/data/config.rs`: interact with `conman` configuration set by the user.
  The configuration will allow the user to specify their upstream. This repo may be hosted
  on any git server.
