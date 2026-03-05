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
├── install.sh               # Bootstrap: detects arch, downloads latest release binary, runs it
├── AGENT.md                 # ← You are here
├── README.md
├── .github/
│   └── workflows/
│       ├── test.yml         # CI: matrix test across Ubuntu, Debian, Fedora, Arch via Docker
│       └── release.yml      # CD: cross-compile musl binaries + publish GitHub Release on v* tag
└── installer/               # Rust TUI project
    ├── Cargo.toml
    ├── Cargo.lock
    └── src/
        ├── main.rs          # Entry point: headless mode (--all/CI=true), event loop, sudo pre-auth, install thread
        ├── app.rs           # App state (component list, cursor, screen, search state, Arc log/done/index)
        ├── ui.rs            # Ratatui rendering (4 screens: Selection, Installing, Report, Search)
        ├── registry.rs      # Static list of all installable components + Group enum + detection logic
        ├── manifest.rs      # Curated mise_registry.toml parser, fuzzy search, runtime fallback
        ├── sys.rs           # Shell helpers: run_cmd, run_cmd_streaming, check_command_exists, get_distro
        └── installer/
            ├── mod.rs       # pub mod declarations
            ├── mise.rs      # Mise self-install + `mise use -g <tools>` orchestration
            ├── system.rs    # Sudo package installs (apt/pacman/dnf) — base-deps + tmux
            └── config.rs    # Dotfile setup: fish config, nushell env.nu, oh-my-tmux, LazyVim + OSC52
```

---

## Architecture & Design Decisions

### How the Installer Works

1. **Bootstrap (`install.sh`)**
   - Detects CPU architecture (`uname -m` → `x86_64` or `aarch64`)
   - Fetches the latest release tag from the GitHub API
   - Downloads `devenv-{version}-{arch}.tar.xz` from GitHub Releases
   - Extracts to a tmp directory and `exec`s `./devenv "$@"`
   - No Rust toolchain required on the user's machine

2. **Headless / non-interactive mode (`main.rs:run_headless`)**
   - Activated by `--all` CLI flag, `CI=true`, or `INSTALLER_ALL=1` env var
   - Skips the TUI entirely — force-selects every component, runs all three phases, prints to stdout
   - Safe to run in CI containers with no TTY

3. **Pre-TUI phase (`main.rs`)**
   - Detects if any `SystemPackage` components are selected
   - If so, runs **`sudo -v` in normal terminal mode** (before `enable_raw_mode`) so the password prompt is visible
   - Spawns a background thread that re-runs `sudo -v` every 50 s to keep credentials cached throughout long installs

3. **TUI selection screen (`ui.rs` + `app.rs`)**
   - On startup `App::new()` calls `sys::check_command_exists()` on every component
   - Already-installed tools default to `Unselected`; existing configs default to `KeepAsIs`
| Group | Items | Icon |
|-------|-------|------|
| `System` | Base Dependencies, Tmux | 🖥️ |
| `Shells` | Fish Shell, Nushell | 🐚 |
| `Editor` | Neovim | ✏️ |
| `Languages` | Rust, Node.js, Go, Python (uv) | 🛠️ |
| `CliTools` | fzf, ripgrep, fd, bat, eza, glow, jaq | 🔧 |
| `Configurations` | fish config, nushell env.nu, oh-my-tmux, LazyVim | ⚙️ |
| `ExtraTools` | Dynamically added via Search screen | 📦 |

  - `app.cursor` indexes into `app.components` only (Option A: headers are purely visual)
  - Keyboard: `↑/↓` or `j/k` — navigate · `Space` — toggle · `a/n` — select all/none · `/` — open search · `Enter` — start · `q` — quit

4. **Installation thread (`main.rs:spawn_installation`)**
   - Runs on a **background thread** so the Ratatui event loop stays responsive
   - Writes log lines into `Arc<Mutex<Vec<String>>>` (shared with the UI)
   - Updates `Arc<Mutex<usize>> install_index` to drive the progress gauge (3 phases)
   - Sets `Arc<Mutex<bool>> install_done = true` when finished → UI switches to Report screen

5. **Three installation phases**
   | Phase | Index | Handler | What it does |
   |-------|-------|---------|--------------|
   | System Packages | 0 | `installer::system` | `sudo apt/pacman/dnf install` base-deps + tmux |
1.  **Bootstrap (`install.sh`)**
    -   Detects CPU architecture (`uname -m` → `x86_64` or `aarch64`)
    -   Fetches the latest release tag from the GitHub API
    -   Downloads `devenv-{version}-{arch}.tar.xz` from GitHub Releases
    -   Extracts to a tmp directory and `exec`s `./devenv "$@"`
    -   No Rust toolchain required on the user's machine

2.  **Headless / non-interactive mode (`main.rs:run_headless`)**
    -   Activated by `--all` CLI flag, `CI=true`, or `INSTALLER_ALL=1` env var
    -   Skips the TUI entirely — force-selects every component, runs all three phases, prints to stdout
    -   Safe to run in CI containers with no TTY

3.  **Pre-TUI phase (`main.rs`)**
    -   Detects if any `SystemPackage` components are selected
    -   If so, runs **`sudo -v` in normal terminal mode** (before `enable_raw_mode`) so the password prompt is visible
    -   Spawns a background thread that re-runs `sudo -v` every 50 s to keep credentials cached throughout long installs

3.  **TUI selection screen (`ui.rs` + `app.rs`)**
    -   On startup `App::new()` calls `sys::check_command_exists()` on every component
    -   Already-installed tools default to `Unselected`; existing configs default to `KeepAsIs`
    | Group | Items | Icon |
    |-------|-------|------|
    | `System` | Base Dependencies, Tmux | 🖥️ |
    | `Shells` | Fish Shell, Nushell | 🐚 |
    | `Editor` | Neovim | ✏️ |
    | `Languages` | Rust, Node.js, Go, Python (uv) | 🛠️ |
    | `CliTools` | fzf, ripgrep, fd, bat, eza, glow, jaq | 🔧 |
    | `Configurations` | fish config, nushell env.nu, oh-my-tmux, LazyVim | ⚙️ |
    | `ExtraTools` | Dynamically added via Search screen | 📦 |

    -   `app.cursor` indexes into `app.components` only (Option A: headers are purely visual)
    -   Keyboard: `↑/↓` or `j/k` — navigate · `Space` — toggle · `a/n` — select all/none · `/` — open search · `Enter` — start · `q` — quit

4.  **Installation thread (`main.rs:spawn_installation`)**
    -   Runs on a **background thread** so the Ratatui event loop stays responsive
    -   Writes log lines into `Arc<Mutex<Vec<String>>>` (shared with the UI)
    -   Updates `Arc<Mutex<usize>> install_index` to drive the progress gauge (3 phases)
    -   Sets `Arc<Mutex<bool>> install_done = true` when finished → UI switches to Report screen

5.  **Three installation phases**
    | Phase | Index | Handler | What it does |
    |-------|-------|---------|--------------|
    | System Packages | 0 | `installer::system` | `sudo apt/pacman/dnf install` base-deps + tmux |
    | Mise Tools | 1 | `installer::mise` | Self-installs `mise`, then `mise use -g <tools>` |
    | Configurations | 2 | `installer::config` | Fish config, Nushell `env.nu`, oh-my-tmux, LazyVim + OSC52 |

### Tool Categories (`registry.rs`)

| Category | Trait | Examples |
|----------|-------|---------|
| `Mise(String)` | zero-sudo, version-managed | rust, node, go, uv, neovim, fish, nu, fzf, rg, fd, bat, eza, glow, jaq |
| `SystemPackage` | requires sudo | base-deps (build-essential etc.), tmux |
| `Config` | bash git/file ops | fish config (with mise shims PATH), nushell `env.nu` (mise shims PATH), oh-my-tmux, LazyVim + OSC52 |

### Search Screen

-   Triggered by `/` on the Selection screen
-   Searches `mise_registry.toml` (100 curated tools, embedded at compile time)
-   If mise is already installed, merges in full `mise registry` output as fallback (deduplicated)
-   `Enter` on a result adds a new `Mise` component in the `ExtraTools` group to the install list
-   Deduplicates: adding the same tool twice is a no-op

All tools that previously used hand-rolled GitHub release downloads have been replaced with `mise`. No direct download code exists anymore.

### Key Design Rules

- Sudo calls are **never made from within the TUI** — only pre-authenticated credentials are used
- Installation is **non-destructive**: configs are backed up (e.g. `~/.config/nvim.bak`) before overwriting
- Tests (`cargo test`) must pass before merging

---

## Outstanding Tasks / Known Issues

> Update this section as work progresses.

- [ ] Enhance dotfiles installation using actual stow/symlink mechanics if current simple bash scripts break across differing Linux flavors (optional).
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

> CI calls `bash install.sh` with `CI=true`, which triggers headless mode in the installer binary. Tool verification uses `~/.local/share/mise/shims` paths.

---

## How to Run Locally

```bash
# Full flow (from scratch)
bash install.sh

# Build and run TUI directly
cd installer && cargo build --release && ./target/release/devenv

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
| 2026-03-03 | Reorganize TUI selection screen into labeled groups: System, Shells, Editor, Languages, CLI Tools, Configurations |
| 2026-03-03 | Add `/` search screen: fuzzy search over 100-tool curated manifest + runtime `mise registry` fallback |
| 2026-03-03 | Add `Group` enum to `registry.rs`; add `manifest.rs` + `mise_registry.toml` (embedded at compile time) |
| 2026-03-03 | Update help bar in TUI with full shortcut guide including new `/` shortcut |
| 2026-03-02 | Replace monolithic `install.sh` (712 lines) with Ratatui TUI installer (`installer/`) |
| 2026-03-02 | Move all tool installs to `mise`; eliminate all direct GitHub release downloads |
| 2026-03-02 | Fix sudo TTY issue: pre-authenticate before TUI, run installation on background thread |
| 2026-03-03 | Add headless/non-interactive mode: `--all` flag, `CI=true`, `INSTALLER_ALL=1` |
| 2026-03-03 | Fix `install.sh` to forward `"$@"` to the installer binary |
| 2026-03-03 | Update CI workflow (`test.yml`) for Ratatui installer + mise shim paths |
| 2026-03-03 | Add `config-nushell`: writes mise shims PATH to `~/.config/nushell/env.nu` |
| 2026-03-03 | Rewrite `README.md` and `README_vi.md` for novice quick-start |
| 2026-03-03 | Remove unused `InstallStatus` variants and `check_args` field; prefix `_stdout` to silence dead code warnings |
| 2026-03-03 | Implement version detection using `mise ls` and standard command versions, display in TUI |
| 2026-03-03 | Remove unused `ureq`, `semver`, and `serde` crates from `Cargo.toml` |
| 2026-03-05 | Rename binary from `installer` to `devenv` in `Cargo.toml` |
| 2026-03-05 | Add `.github/workflows/release.yml`: cross-compile musl binaries for x86_64 + aarch64, publish GitHub Release on `v*` tag or `workflow_dispatch` |
| 2026-03-05 | Rewrite `install.sh` as thin download bootstrap (~60 lines): detects arch, downloads release binary, no Rust required |
| 2026-03-05 | Update `README.md` with `curl \| tar` one-liner quick start and supported architectures table |
