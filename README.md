# devenv-linux

`devenv-linux` is a small Linux development environment bootstrapper with an interactive TUI installer built in Rust. It installs a curated setup with tools managed by [mise](https://mise.jdx.dev), including Fish, Bash config helpers, Neovim, common CLI tools, and Zellij.

Supported distros: Ubuntu 24.04, Debian 13, Fedora 43, and Arch Linux.

## Quick Start

Run the bootstrap script:

```bash
curl -fsSL https://raw.githubusercontent.com/nguyenvulong/devenv-linux/main/install.sh | bash
```

Or download and run the latest release binary directly:

```bash
curl -fsSL https://github.com/nguyenvulong/devenv-linux/releases/latest/download/devenv-$(uname -m).tar.xz | tar -xJ
./devenv
```

After installation, reload your shell:

```bash
source ~/.bashrc
source ~/.config/fish/config.fish
```

## Headless / CI Mode

Skip the TUI and install everything automatically:

```bash
./devenv --all
# or
CI=true ./devenv
```

## License

MIT
