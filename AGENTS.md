# AGENT.md — devenv-linux Context Guide

> This file is the authoritative context document for AI agents working on this repo.
> Read it before making any changes. Update relevant sections after completing work.

---

## Project Overview

**devenv-linux** is a personal Linux development environment bootstrapper.
It installs a curated set of CLI tools, programming language toolchains, shells, editors, and their configurations onto a fresh Linux machine.

**Entry point:** `install.sh` (a thin bash bootstrap)  
**UI:** An interactive Ratatui TUI written in Rust (under `installer/`)  
**Package management:** [`mise-en-place`](https://mise.jdx.dev) is used for all versioned tools  
**Target distros:** Ubuntu/Debian, Arch Linux, Fedora/RedHat

---

## Directory Structure

```
devenv-linux/
├── install.sh               # Bootstrap: installs Rust, builds the TUI, launches it
├── AGENT.md                 # ← You are here
├── README.md
├── .github/
│   └── workflows/
│       └── test.yml         # CI: matrix test across Ubuntu, Debian, Fedora, Arch via Docker
└── installer/               # Rust TUI project
    ├── Cargo.toml
    ├── Cargo.lock
    └── src/
        ├── main.rs          # Entry point, event loop, sudo pre-auth, install thread
        ├── app.rs           # App state (component list, cursor, screen, Arc log/done/index)
        ├── ui.rs            # Ratatui rendering (3 screens: Selection, Installing, Report)
        ├── registry.rs      # Static list of all installable components + detection logic
        ├── sys.rs           # Shell helpers: run_cmd, run_cmd_streaming, check_command_exists, get_distro
        └── installer/
            ├── mod.rs       # pub mod declarations
            ├── mise.rs      # Mise self-install + `mise use -g <tools>` orchestration
            ├── system.rs    # Sudo package installs (apt/pacman/dnf) — base-deps + tmux
            └── config.rs    # Dotfile setup: fish config, oh-my-tmux, LazyVim + OSC52
```

---

## Architecture & Design Decisions

### How the Installer Works

1. **Bootstrap (`install.sh`)**
   - Installs minimal system deps (curl, git, gcc) if missing
   - Installs `rustup` if `cargo` is not available
   - Runs `cargo build --release` inside `installer/`
   - Executes `./installer/target/release/installer`

2. **Pre-TUI phase (`main.rs`)**
   - Detects if any `SystemPackage` components are selected
   - If so, runs **`sudo -v` in normal terminal mode** (before `enable_raw_mode`) so the password prompt is visible
   - Spawns a background thread that re-runs `sudo -v` every 50 s to keep credentials cached throughout long installs

3. **TUI selection screen (`ui.rs` + `app.rs`)**
   - On startup `App::new()` calls `sys::check_command_exists()` on every component
   - Already-installed tools default to `Unselected`; existing configs default to `KeepAsIs`
   - Keyboard: `↑/↓` or `j/k` — navigate · `Space` — toggle · `a/n` — select all/none · `Enter` — start · `q` — quit
   - Config toggling cycles: `Selected ➜ KeepAsIs ➜ Unselected`

4. **Installation thread (`main.rs:spawn_installation`)**
   - Runs on a **background thread** so the Ratatui event loop stays responsive
   - Writes log lines into `Arc<Mutex<Vec<String>>>` (shared with the UI)
   - Updates `Arc<Mutex<usize>> install_index` to drive the progress gauge (3 phases)
   - Sets `Arc<Mutex<bool>> install_done = true` when finished → UI switches to Report screen

5. **Three installation phases**
   | Phase | Index | Handler | What it does |
   |-------|-------|---------|--------------|
   | System Packages | 0 | `installer::system` | `sudo apt/pacman/dnf install` base-deps + tmux |
   | Mise Tools | 1 | `installer::mise` | Self-installs `mise`, then `mise use -g <tools>@latest` |
   | Configurations | 2 | `installer::config` | Fish config, oh-my-tmux clone, LazyVim clone + OSC52 |

### Tool Categories (`registry.rs`)

| Category | Trait | Examples |
|----------|-------|---------|
| `Mise(String)` | zero-sudo, version-managed | rust, node, go, uv, neovim, fish, nu, fzf, rg, fd, bat, eza, glow, jaq |
| `SystemPackage` | requires sudo | base-deps (build-essential etc.), tmux |
| `Config` | bash git/file ops | fish config, oh-my-tmux, LazyVim + OSC52 |

All tools that previously used hand-rolled GitHub release downloads have been replaced with `mise`. No direct download code exists anymore.

### Key Design Rules

- `.rs` files must stay **< 500 lines** — split into modules if approaching this limit
- Every module has a **single responsibility** (see directory structure above)
- Sudo calls are **never made from within the TUI** — only pre-authenticated credentials are used
- Installation is **non-destructive**: configs are backed up (e.g. `~/.config/nvim.bak`) before overwriting
- Tests (`cargo test`) must pass before merging

---

## Outstanding Tasks / Known Issues

> Update this section as work progresses.

- [ ] **Version detection** — the initial scan only checks if a binary exists, not its version. Wire up `get_installed_version()` in `sys.rs` to populate `InstallStatus::Installed(version_string)` properly and show `UpdateAvailable` when mise has a newer release.
- [ ] **CI workflow update** — `test.yml` expects `~/.local/bin/fzf` etc. (old direct-download paths). These paths need updating to `mise` shim paths (`~/.local/share/mise/shims/`) or by sourcing `mise activate` before verification.
- [ ] **Non-interactive / headless mode** — the TUI requires a TTY. Add a `--non-interactive` or `--all` flag that skips the TUI and installs everything for CI/scripted use.
- [ ] **`ureq` / `semver` crates unused** — they were planned for version comparison via GitHub API but not yet implemented. Either implement version checking or remove from `Cargo.toml`.
- [ ] **Fish & Nushell PATH** — mise shims must be on PATH for fish/nu configs to reference them correctly. Ensure `~/.local/share/mise/shims` is appended in the fish config written by `config.rs`.

---

## CI / Testing

GitHub Actions workflow: [`.github/workflows/test.yml`](.github/workflows/test.yml)

Runs on push/PR to `dev` branch. Tests across 4 containers:

| Job | Image | Run as |
|-----|-------|--------|
| ubuntu-24.04 | `ubuntu:24.04` | non-root (testuser, NOPASSWD sudo) |
| debian-13 | `debian:trixie` | non-root |
| fedora-43 | `fedora:43` | root |
| arch | `archlinux:latest` | non-root |

> **Note:** CI currently tests the old `install.sh` flow. It needs updating to handle the Ratatui TUI non-interactively (see "Non-interactive mode" task above).

---

## How to Run Locally

```bash
# Full flow (from scratch)
bash install.sh

# Build and run TUI directly
cd installer && cargo build --release && ./target/release/installer

# Quick compile check
cd installer && cargo check

# Run unit tests
cd installer && cargo test
```

---

## Branch Strategy

| Branch | Purpose |
|--------|---------|
| `dev` | Active development — all feature work goes here |
| `main` | Stable releases only |

Commits should follow Conventional Commits format: `feat:`, `fix:`, `chore:`, `docs:`, etc.

---

## Changelog

| Date | Change |
|------|--------|
| 2026-03-02 | Replace monolithic `install.sh` (712 lines) with Ratatui TUI installer (`installer/`) |
| 2026-03-02 | Move all tool installs to `mise`; eliminate all direct GitHub release downloads |
| 2026-03-02 | Fix sudo TTY issue: pre-authenticate before TUI, run installation on background thread |
