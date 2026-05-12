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

There are two non-interactive install modes.

Install every built-in component. This is useful for CI images and disposable
development environments:

```bash
./devenv --all
# or
CI=true ./devenv
```

Install from a prepared config file when you want to choose components or pin
mise-managed tool versions:

```bash
./devenv --config devenv.example.toml
# or
./devenv -c devenv.example.toml
```

Start from `devenv.example.toml`, which includes every built-in component. Only
entries with `enabled = true` are installed. Omit `version` or set it to
`"latest"` to keep the default mise behavior. Pinned versions apply only to
mise-managed tools; system packages and configuration tasks do not accept a
`version` field.

```toml
[[components]]
id = "rust"
enabled = true
version = "1.85.0"

[[components]]
id = "config-fish"
enabled = true
```

The installer validates config files before installing. Unknown component IDs,
duplicate component entries, empty versions, and `version` fields on non-mise
components fail with an error.

## License

MIT
