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
2. `main.rs` enters full headless mode when `--all`, `CI=true`, or `INSTALLER_ALL=1` is set.
3. `main.rs` enters config-driven headless mode when `--config <path>`, `--config=<path>`, or `-c <path>` is set.
4. Config-driven headless mode reads TOML from `headless_config.rs`, selects only enabled component IDs, and applies pinned versions only to `mise` tools.
5. The TUI loads the component list, detects already-installed tools, and loads the searchable `mise` manifest.
6. When installation starts, `sudo -v` runs in the normal terminal only if selected system packages need it.
7. Installation runs in 3 phases:
   - system packages
   - mise tools
   - configurations

## Current Product Direction

- Terminal multiplexer: `zellij`
- Shell configs: `bash` and `fish`
- No `tmux` or `nushell`
- Search uses embedded `mise_registry.toml`, with runtime `mise registry` fallback when available
- Config-driven headless installs use TOML component IDs from `devenv.example.toml`; mise tool versions default to `latest`

## Key Implementation Notes

- Sudo must never be requested on startup for the TUI path; request it only when selected system packages require it.
- Config installs should be non-destructive and back up existing user files when overwriting.
- `devenv.example.toml` should include every built-in component from `registry.rs`.
- Config-driven headless installs must reject unknown component IDs, duplicate entries, empty versions, and versions on non-`mise` components.
- `mise_version: None` means install `@latest`; pinned versions should produce `mise use -g <tool>@<version>`.
- Install logs are shared through `Arc<Mutex<Vec<String>>>`.
- Install progress uses atomics: `install_done: AtomicBool` and `install_index: AtomicUsize`.
- Keep installer code simple and explicit; prefer fallible helpers over panics.

## Run Locally

```bash
bash install.sh

cd installer
cargo build --release
./target/release/devenv
./target/release/devenv --config ../devenv.example.toml

cargo test
cargo clippy --all-targets --all-features --locked -- -D warnings
```

## Branches

- `dev`: active development
- `main`: stable releases

Use Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`.
