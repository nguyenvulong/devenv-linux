# Linux Development Environment Setup

A streamlined script to set up a modern development environment on Linux systems, focusing on installing the latest versions of essential tools directly from GitHub into `$HOME/.local/`.

## Overview

This script installs and configures:

- **Shell Utilities**: fzf, ripgrep, fd, tmux, bat, eza
- **Programming Languages**: Rust, Node.js (via nvm), Go
- **Development Tools**: Neovim (with LazyVim), Nushell
- **Package Managers**: uv (Python)

### Demo
[![asciicast](https://asciinema.org/a/708631.svg)](https://asciinema.org/a/708631)

### Why `$HOME/.local/`?

- No root privileges needed for updates
- Always get the latest releases from GitHub
- Avoid conflicts with system package manager
- Consistent across different Linux distributions

## Usage

#### Clone the repository:
```bash
git clone https://github.com/nguyenvulong/devenv-linux.git
```
#### Run the installation
```bash
cd devenv-linux
chmod +x install.sh
./install.sh
```
#### After installation
```
source $HOME/.bashrc
nvim # Wait for plugin installation to complete
```
To exit Neovim: press `<space> q q`

## Notes
- Tested on: Debian, Ubuntu, Fedora, CentOS Stream, and Arch Linux
- Must be run from `bash` shell
- Need sudo privileges to install `build essentials`, `tmux`, and `nushell` 
- Can be overwhelming for new users
- Some tools can be redundant 

## Future plan
- More distros
- Personalization
- Dockerization

## License
MIT