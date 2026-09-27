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
├── docs/
│   └── images/          # README screenshots (SVG)
├── .github/
│   ├── scripts/
│   │   ├── verify-install.sh
│   │   └── verify-shell-path.sh
│   └── workflows/
│       ├── test.yml
│       └── release.yml
└── installer/
    ├── Cargo.toml
    ├── Cargo.lock
    └── src/
        ├── main.rs
        ├── app.rs
        ├── executor.rs
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

1. `install.sh` detects architecture, checks for curl/wget, tar, and gzip (printing a distro-specific install command when missing), downloads `<arch>.tar.gz` (falling back to `<arch>.tar.xz`, which needs xz, for releases up to v1.1.1) and `SHA256SUMS` from `/releases/latest/download/` (or the `DEVENV_VERSION` tag, or `DEVENV_DOWNLOAD_URL`), verifies the checksum, extracts, and runs `devenv` without `exec` so its temporary directory is cleaned up.
2. `main.rs` parses arguments strictly: `--help`/`-h` and `--version`/`-v` win; unknown, duplicate, or conflicting (`--all` with `--config`) arguments exit with status 2.
3. `main.rs` enters full headless mode for `--all` or `INSTALLER_ALL=1`. `CI=true` never triggers an install.
4. `main.rs` enters config-driven headless mode when `--config <path>`, `--config=<path>`, or `-c <path>` is set.
5. Config-driven headless mode reads TOML from `headless_config.rs`, selects only enabled component IDs, and applies pinned versions only to `mise` tools.
6. The TUI refuses to start without a terminal on stdout or when `CI=true`. It installs a panic hook that restores the terminal, draws a loading screen, then probes PATH tools, existing configs, and globally configured mise versions concurrently, and loads the searchable `mise` manifest.
7. Every TUI component starts at Keep. Users explicitly choose Install or, for globally configured mise tools only, Deactivate.
8. Enter opens a review containing only planned mutations, kept count, implicit prerequisites (mise and missing system prerequisites), a warning when sudo is unavailable, and replacement warnings. A second Enter confirms execution.
9. After review confirmation, `sudo -v` runs in the normal terminal only if planned system packages (selected Build Tools or missing prerequisites) need it and the user is not root.
10. `executor.rs` runs installation in 3 phases for both the TUI and headless modes and records each component outcome independently:
   - system packages
   - mise tools
   - configurations
11. The TUI event loop always polls input with a timeout (never busy-waits). After installation the log stays visible until Enter opens the summary, which ends with "Next steps" (also printed by headless modes).

## Current Product Direction

- Terminal multiplexer: `zellij`
- Shell configs: `bash` and `fish`
- No `tmux` or `nushell`
- Search uses embedded `mise_registry.toml`, with runtime `mise registry` fallback when available
- Config-driven headless installs use TOML component IDs from `devenv.example.toml`; mise tool versions default to `latest`
- Release assets are named by architecture only (`x86_64.tar.gz`, `aarch64.tar.gz`, plus the same binaries as `.tar.xz` for older links) so `/releases/latest/download/...` URLs stay stable across versions. Every release also publishes `SHA256SUMS` and build provenance attestations; `install.sh` refuses to run an unverified archive.

## Key Implementation Notes

- Sudo must never be requested on startup for the TUI path; request it only after review confirmation when a planned system-package installation requires it.
- Opening the TUI and pressing Enter without changing the default Keep actions must perform no writes.
- TUI actions are Keep, Install, and Deactivate. Space toggles Keep/Install and returns Deactivate to Keep; there is no bulk-deactivate action.
- Deactivate is available only when `mise ls --global --json <tool>` proves global ownership. Remove every requested global version with `mise unuse --global --no-prune <tool@version>`.
- PATH-only and locally configured tools cannot be deactivated. System packages and configurations have no removal behavior.
- System prerequisites are implicit, never components: `InstallPlan::for_environment` adds the ones the plan needs and the machine lacks (curl + CA certificates to bootstrap mise when missing; git + a C compiler for LazyVim; a C compiler for Rust). Phase 1 installs them with the package manager together with the optional Build Tools (`base-deps`). `InstallPlan::from_components` stays pure for tests.
- Without root or sudo, the review warns up front and the system phase fails with a clear message instead of crashing.
- Mise installation is lazy. It is an implicit prerequisite only for selected mise-tool installs and Bash/Fish configuration.
- Headless installs record and print per-component outcomes, continue independent work after failures, and exit nonzero if any component fails.
- Arch package installation uses existing databases with `pacman -S --needed`; users must complete a full system upgrade separately when databases/packages are stale. Never run a standalone `pacman -Sy`.
- Root installations call package managers directly and do not require sudo.
- Distro detection parses quoted ID values and whitespace-separated ID_LIKE tokens, including rhel.
- Mise bootstrap downloads successfully to a temporary file before execution and verifies the resulting executable. It requires `curl` and fails early with a clear message when it is missing; LazyVim similarly requires `git`.
- Log the mise version (honoring `MISE_VERSION`) and the LazyVim starter commit for reproducibility.
- Command detection requires the executable bit and checks the mise shims directory resolved from `MISE_DATA_DIR`, then `$XDG_DATA_HOME/mise`, then `~/.local/share/mise`. Root detection uses `geteuid()`.
- Shell setup is implicit whenever a plan installs mise tools (or selects Bash/Fish configuration) and mise is ready: `~/.bashrc` gets interactive activation, the login profile (`~/.bash_profile`, `~/.bash_login`, or `~/.profile`, whichever bash reads) gets `~/.local/bin` plus the mise shims directory, and fish users get `~/.config/fish/conf.d/devenv-mise.fish` unless `config.fish` already activates mise. "Bash Configuration" is this same setup, selectable on its own. If shell setup fails, the mise tools installed in that run are reported as failed.
- Every block devenv adds to a user file sits between `# >>> devenv-linux >>>` and `# <<< devenv-linux <<<` with a comment explaining it; keep this so blocks can be detected and removed.
- "Fish Configuration" writes defaults to `config.fish` only when it does not exist; activation lives in the conf.d file. Never change the user's login shell.
- Shell activation resolves mise on PATH with a ~/.local/bin fallback and is skipped when mise is missing. Fish uses interactive activation and noninteractive --shims; Bash hooks run only interactively.
- Explicit shell configuration installs migrate exact legacy installer activation lines with numbered backups, ignore commented activation when detecting setup, and preserve custom activation blocks.
- Config installs should be non-destructive and back up existing user files when overwriting. Shell config writes are atomic (temporary sibling + rename), preserve permissions, and follow symlinks so dotfile-manager links stay intact.
- Default Fish config uses `fish_add_path --append` and defines colors, aliases, and the history wrapper only in interactive shells.
- Explicit Neovim configuration installs must prepare a staging directory before touching the live config, use numbered `nvim.bak` backups, and restore the original if the final swap fails.
- `devenv.example.toml` should include every built-in component from `registry.rs`.
- Config-driven headless installs must reject unknown component IDs, duplicate entries, empty versions, and versions on non-`mise` components.
- `mise_version: None` means install `@latest`; pinned versions should produce `mise use -g <tool>@<version>`.
- Headless `enabled = true` maps to Install and `false` maps to Keep. Headless modes cannot Deactivate.
- Install logs are shared through `Arc<Mutex<Vec<String>>>`.
- Install progress uses atomics: `install_done: AtomicBool` and `install_index: AtomicUsize`.
- Reports must use recorded per-component outcomes: Succeeded, Failed, Already configured, Deactivated, or Kept.
- Keep installer code simple and explicit; prefer fallible helpers over panics. Installer-thread panics are caught and reported.

## Documentation

- Keep `README.md` short: pitch, one-line quick start, what you get, brief non-interactive usage.
- `docs/images/*.svg` are captured from real terminal sessions (the TUI, fish, bat, fzf, zellij) and rendered to SVG. Recapture them when the TUI layout changes noticeably; do not hand-edit or fabricate output.

## Run Locally

```bash
bash install.sh

cd installer
cargo build --release
./target/release/devenv
./target/release/devenv --help
./target/release/devenv --version
./target/release/devenv --config ../devenv.example.toml

cargo fmt --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --locked
shellcheck ../install.sh ../.github/scripts/*.sh
```

CI (`test.yml`) runs a `lint` job with the commands above plus an end-to-end `install.sh` checksum test, then a `bare` job on plain Debian and Fedora images with only curl installed (checks automatic prerequisites and PATH for root and a `su` user via `.github/scripts/verify-shell-path.sh`), and the per-distro `--all` install matrix, which verifies results with `.github/scripts/verify-install.sh`.
Scripts under `.github/scripts/` start with a header comment: purpose, usage, and what they check.

## Branches

- `dev`: active development
- `main`: stable releases

Before releasing, merge `origin/main` into `dev`, then bump the package version in both `installer/Cargo.toml` and `installer/Cargo.lock` to an unused version. CI runs on `dev` pushes and PRs targeting `dev` or `main`. After checks pass, merge the `dev` → `main` release PR with a merge commit (never squash or rebase), create an annotated `v<version>` tag on that merged commit, and push the tag to publish the release. Merge the released `main` back into `dev` to preserve shared ancestry and version consistency.

Feature branches target `dev` and are squash-merged, one commit per logical change. Only the `dev` → `main` release PR uses a merge commit.

Use Conventional Commits: `feat:`, `fix:`, `docs:`, `chore:`, `ci:`.
