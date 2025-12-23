# Linux Development Environment Setup

A streamlined script to set up a modern development environment on Linux systems, focusing on installing the latest versions of essential tools directly from GitHub into `$HOME/.local/`.

## Overview

This script installs and configures:

- **Shell Utilities**: fzf, ripgrep, fd, tmux, bat, eza, glow, jaq
- **Programming Languages**: Rust, Node.js (via nvm), Go
- **Development Tools**: Neovim (with LazyVim), Nushell, Fish
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

For convenience, you can add some aliases, here are mine

##### Bash

```bash
# Aliases added to `~/.bashrc`
alias v="nvim"
alias vd="nvim -d"
alias cat="BAT_THEME=DarkNeon bat --paging=never --plain"
alias ls="eza --icons=always"
alias ll="eza -lh"
alias l="eza -lah --classify --grid"
alias tree="eza --tree"
```

And `source $HOME/.bashrc` again.

##### Fish
```bash
#
set PATH $PATH /home/longnv/.local/bin

# aliases
alias ls='eza --icons=always'
alias la='ls -a'
alias ll='eza -lah'
alias l='eza -lah --classify --grid'

alias vim='v'
alias v='nvim'
alias vd='nvim -d'
alias cat='BAT_THEME=Dracula bat --paging=never --plain'

function history
    builtin history --show-time="%Y-%m-%d %H:%M:%S " $argv
end
```

And `source ~/.config/fish/config.fish`.

## Notes
- Tested on: Debian, Ubuntu, Fedora, CentOS Stream, and Arch Linux
- Must be run from `bash` shell
- Need `sudo` privileges to install `build essentials`, `tmux`, and `nushell` 
- Can be overwhelming for new users
- Some tools can be redundant 

## Future plan
- More distros
- Dockerization

## License
MIT
