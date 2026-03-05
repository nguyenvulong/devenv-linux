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

Download and run the latest pre-built binary — no Rust toolchain required:

```bash
# x86_64
curl -fsSL https://github.com/nguyenvulong/devenv-linux/releases/latest/download/devenv-$(uname -m).tar.xz | tar -xJ && ./devenv

# aarch64 (ARM64)
curl -fsSL https://github.com/nguyenvulong/devenv-linux/releases/latest/download/devenv-aarch64.tar.xz | tar -xJ && ./devenv
```

Or use the bootstrap script (auto-detects architecture):

```bash
curl -fsSL https://raw.githubusercontent.com/nguyenvulong/devenv-linux/main/install.sh | bash
```

> **Requires:** `curl` or `wget`, and `sudo` access (for system packages & tmux).

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
./devenv --all
# or
CI=true ./devenv
```

## Supported Distros

Ubuntu 24.04 · Debian 13 · Fedora 43 · Arch Linux

## Supported Architectures

| Architecture | Binary |
|---|---|
| x86_64 (Intel/AMD) | `devenv-{version}-x86_64.tar.xz` |
| aarch64 (ARM64) | `devenv-{version}-aarch64.tar.xz` |

## Building from Source

If pre-built binaries don't work for your environment:

```bash
git clone https://github.com/nguyenvulong/devenv-linux.git
cd devenv-linux
bash install.sh   # bootstraps via release binary

# Or compile manually:
cd installer && cargo build --release && ./target/release/devenv
```

## License

MIT
