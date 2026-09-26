# devenv-linux

`devenv-linux` is a small Linux development environment bootstrapper with an interactive TUI installer built in Rust. It installs a curated setup with tools managed by [mise](https://mise.jdx.dev), including Fish, Bash config helpers, Neovim, common CLI tools, and Zellij.

Supported distros: Ubuntu 24.04, Debian 13, Fedora 43, and rolling Arch Linux.

## Quick Start

Run the bootstrap script:

```bash
curl -fsSL https://raw.githubusercontent.com/nguyenvulong/devenv-linux/main/install.sh | bash
```

The script downloads the release archive for your architecture, verifies it
against the release's `SHA256SUMS`, and refuses to run a binary that does not
match. It needs `curl` or `wget`, plus `tar` and `xz`. Set `DEVENV_VERSION` to
install a specific release:

```bash
curl -fsSL https://raw.githubusercontent.com/nguyenvulong/devenv-linux/main/install.sh | DEVENV_VERSION=v1.1.0 bash
```

Or download, verify, and run the latest release binary directly:

```bash
base=https://github.com/nguyenvulong/devenv-linux/releases/latest/download
curl -fsSLO "$base/$(uname -m).tar.xz" -fsSLO "$base/SHA256SUMS"
sha256sum -c --ignore-missing SHA256SUMS
tar xJf "$(uname -m).tar.xz"
./devenv
```

Release archives also carry GitHub build provenance, which you can check with
`gh attestation verify "$(uname -m).tar.xz" --repo nguyenvulong/devenv-linux`.

Common CLI helpers:

```bash
./devenv --help
./devenv -h
./devenv --version
./devenv -v
```

Unknown or conflicting arguments are rejected with exit code 2. Without
`--all` or `--config`, devenv needs an interactive terminal and exits with an
error otherwise.

After installation, reload your shell:

```bash
source ~/.bashrc
source ~/.config/fish/config.fish
```

## Interactive Actions

The interactive installer starts every component at **Keep**. Opening the TUI
and pressing Enter without choosing an action does not write anything.

Each component has one explicit desired action:

- `[=]` **Keep** leaves the component unchanged.
- `[+]` **Install** installs a missing component, updates a globally managed
  mise tool to latest, or explicitly reruns an existing configuration.
- `[-]` **Deactivate** removes each requested version from the global mise
  configuration. It does not delete cached installations and never changes
  local project configurations. This action is unavailable for PATH-only tools.

Use `j`/`k` or the arrow keys to navigate. The action shortcuts are `i` for
Install, `u` for Keep, and `d` for Deactivate. Space safely toggles Keep and
Install; when the current action is Deactivate, Space returns it to Keep. Use
`a` to Install all, `n` to Keep all, and `/` to add a tool from the mise
registry with Install intent. There is no bulk-deactivate shortcut.

Enter opens a review containing only planned mutations, the number of kept
components, implicit prerequisites, and replacement warnings. Enter again
confirms execution; Esc returns to selection. Sudo is requested only after
confirmation and only when a planned system-package installation needs it.

The review also lists commands the plan relies on but does not install:
`curl` for bootstrapping mise and `git` for cloning LazyVim. Select **Base
Dependencies** or install them first; otherwise those components fail early
with a clear message. After installation the log stays on screen until Enter
opens the summary.

Mise is installed lazily when an Install action needs a mise tool or Bash/Fish
activation. Deactivation uses the versions found in the global mise config and
leaves installed caches in place. The installer logs the mise version it
installed or found; set `MISE_VERSION` to pin the mise bootstrap version.

Explicitly reinstalling the LazyVim configuration is staged before the live
configuration is touched. Once staging succeeds, the existing directory is
renamed to `~/.config/nvim.bak`, then `.bak.1`, `.bak.2`, and so on. If the
final swap fails, the original configuration is restored. The installed
LazyVim starter commit is logged.

Bash and Fish configuration updates are written atomically (temporary file,
then rename) after a numbered backup, keep file permissions, and follow
symlinks from dotfile managers. The default Fish config adds mise paths with
`fish_add_path` and only defines aliases in interactive shells.

## Non-Interactive Install Modes

### Install Everything

Install every built-in component. This is useful for CI images and disposable
development environments:

```bash
./devenv --all
# or
INSTALLER_ALL=1 ./devenv
```

`CI=true` alone no longer triggers a full install; pass `--all` explicitly.

### Install From Config

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
