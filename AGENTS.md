# AGENTS.md -- devenv-linux Context Guide

Read this file before changing code. Update it when installer behavior or workflow changes.

## Project Overview

`devenv-linux` bootstraps a Linux development environment.

- Entry point: `install.sh`
- Installer UI: Rust + Ratatui in `installer/`
- Tool management: `mise`
- Target distros: Ubuntu/Debian, Arch, Fedora/RedHat

## Repository Layout

```text
devenv-linux/
├── install.sh
├── AGENTS.md
├── DEVELOPMENT.md
├── README.md
├── devenv.example.toml
├── .github/workflows/
│   ├── test.yml
│   └── release.yml
└── installer/
    ├── Cargo.toml
    ├── Cargo.lock
    └── src/
        ├── main.rs
        ├── app.rs
        ├── ui.rs
        ├── theme.rs
        ├── registry.rs
        ├── manifest.rs
        ├── headless_config.rs
        ├── sys.rs
        └── installer/
            ├── mod.rs
            ├── mise.rs
            ├── system.rs
            └── config.rs
```

## Installer Flow

1. `install.sh` detects architecture, downloads the latest release binary, extracts it, and runs `./devenv`.
2. `main.rs` prints helper output and exits for `--help`/`-h` and `--version`/`-v`.
3. `main.rs` enters full headless mode when `--all`, `CI=true`, or `INSTALLER_ALL=1` is set.
4. `main.rs` enters config-driven headless mode when `--config <path>`, `--config=<path>`, or `-c <path>` is set.
5. Config-driven headless mode reads TOML from `headless_config.rs`, selects only enabled component IDs, and applies pinned versions only to `mise` tools.
6. The TUI loads the component list, observes PATH tools, existing configs, and globally configured mise versions, and loads the searchable `mise` manifest.
7. Every TUI component starts at Keep. Users explicitly choose Install or, for globally configured mise tools only, Deactivate.
8. Enter opens a review containing only planned mutations, kept count, implicit prerequisites, and replacement warnings. A second Enter confirms execution.
9. After review confirmation, `sudo -v` runs in the normal terminal only if planned system-package installations need it.
10. Installation runs in 3 phases and records each component outcome independently:
   - system packages
   - mise tools
   - configurations

## Current Product Direction

- Terminal multiplexer: `zellij`
- Shell configs: `bash` and `fish`
- No `tmux` or `nushell`
- Search uses embedded `mise_registry.toml`, with runtime `mise registry` fallback when available
- Config-driven headless installs use TOML component IDs from `devenv.example.toml`; mise tool versions default to `latest`
- Release assets are named by architecture only (`x86_64.tar.xz`, `aarch64.tar.xz`) so `/releases/latest/download/...` URLs stay stable across versions.

## Key Implementation Notes

- Sudo must never be requested on startup for the TUI path; request it only after review confirmation when a planned system-package installation requires it.
- Opening the TUI and pressing Enter without changing the default Keep actions must perform no writes.
- TUI actions are Keep, Install, and Deactivate. Space toggles Keep/Install and returns Deactivate to Keep; there is no bulk-deactivate action.
- Deactivate is available only when `mise ls --global --json <tool>` proves global ownership. Remove every requested global version with `mise unuse --global --no-prune <tool@version>`.
- PATH-only and locally configured tools cannot be deactivated. System packages and configurations have no removal behavior.
- Mise installation is lazy. It is an implicit prerequisite only for selected mise-tool installs and Bash/Fish configuration.
- Config installs should be non-destructive and back up existing user files when overwriting.
- Explicit Neovim configuration installs must prepare a staging directory before touching the live config, use numbered `nvim.bak` backups, and restore the original if the final swap fails.
- `devenv.example.toml` should include every built-in component from `registry.rs`.
- Config-driven headless installs must reject unknown component IDs, duplicate entries, empty versions, and versions on non-`mise` components.
- `mise_version: None` means install `@latest`; pinned versions should produce `mise use -g <tool>@<version>`.
- Headless `enabled = true` maps to Install and `false` maps to Keep. Headless modes cannot Deactivate.
- Install logs are shared through `Arc<Mutex<Vec<String>>>`.
- Install progress uses atomics: `install_done: AtomicBool` and `install_index: AtomicUsize`.
- Reports must use recorded per-component outcomes: Succeeded, Failed, Already configured, Deactivated, or Kept.
- Keep installer code simple and explicit; prefer fallible helpers over panics.

## Run Locally

```bash
bash install.sh

cd installer
cargo build --release
./target/release/devenv
./target/release/devenv --help
./target/release/devenv --version
./target/release/devenv --config ../devenv.example.toml

cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

## Branches

- `dev`: active development
- `main`: stable releases

Use Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`.
