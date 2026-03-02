# Linux Development Environment Setup

An interactive TUI installer that sets up a modern development environment on Linux.  
Built with Rust + [Ratatui](https://github.com/ratatui/ratatui). Tools are managed by [mise](https://mise.jdx.dev).

## What Gets Installed

| Category | Tools |
|---|---|
| Shell utilities | fzf, ripgrep, fd, bat, eza, glow, jaq |
| Terminal | tmux (+ oh-my-tmux) |
| Languages | Rust, Node.js, Go, Python (uv) |
| Editor | Neovim (LazyVim) |
| Shells | Fish, Nushell |

## Quick Start

```bash
git clone https://github.com/nguyenvulong/devenv-linux.git
cd devenv-linux
bash install.sh
```

`install.sh` will:
1. Install build essentials & Rust (if missing)
2. Compile the TUI installer
3. Launch an interactive menu — pick what you want, press **Enter**

> **Requires:** `bash`, `curl`, `git`, and `sudo` access (for system packages & tmux).

## After Installation

Reload your shell:

```bash
source ~/.bashrc          # bash
source ~/.config/fish/config.fish   # fish
```

Open Neovim once to finish plugin setup:

```bash
nvim
# Wait for plugins to install, then press <Space>qq to quit
```

## Headless / CI Mode

Skip the TUI and install everything automatically:

```bash
bash install.sh --all
# or
CI=true bash install.sh
```

## Supported Distros

Ubuntu 24.04 · Debian 13 · Fedora 43 · Arch Linux

## License

MIT
