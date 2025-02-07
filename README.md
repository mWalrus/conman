# ConMan
Easily manage local configuration across installations and devices by leveraging git.

## Principles
- ConMan should be straight forward
- ConMan should be intuitive
- ConMan should be descriptive
- ConMan should be as easy to use as possible
- ConMan should provide encryption for secure file storage

## Installation
1. Create a repo with your favorite git provider (GitHub, GitLab, ...)
2. Clone the project: `git clone git@github.com:mwalrus/conman`
3. Install ConMan: `make install`
<!-- add example config.toml -->
4. Populate `config.toml` with the required information
5. `conman init` to clone down your configuration storage specified in your `config.toml`
6. `conman help` for more information and usage

