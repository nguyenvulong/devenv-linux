//! Undo what devenv changed in the user's home directory.
//!
//! The default uninstall removes only devenv's shell changes: the marked
//! `# >>> devenv-linux >>>` blocks, the fish conf.d activation file, the
//! unmarked activation written by releases up to v1.1.1, and a `config.fish`
//! that is still exactly devenv's default. `purge` also removes mise and
//! everything it installed. Neovim configs and system packages are never
//! touched; the plan carries reminders about them instead.

use super::config::{
    BLOCK_BEGIN, BLOCK_END, FISH_ACTIVATION_FILE, FISH_DEFAULTS, next_backup_path, write_atomically,
};
use anyhow::{Context, Result, anyhow};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Activation blocks written by releases up to v1.1.1, before markers.
const LEGACY_BLOCKS: [&str; 2] = [
    r#"# mise activation -- added by devenv-linux installer
if ! command -v mise >/dev/null 2>&1; then
    export PATH="$HOME/.local/bin:$PATH"
fi
case $- in
    *i*) eval "$(mise activate bash)" ;;
esac
"#,
    r#"# mise activation -- added by devenv-linux installer
if not type -q mise
    set -gx PATH "$HOME/.local/bin" $PATH
end
if status is-interactive
    mise activate fish | source
else
    mise activate fish --shims | source
end
"#,
];

/// Single activation lines written by the first releases.
const LEGACY_LINES: [&str; 2] = [
    "eval \"$($HOME/.local/bin/mise activate bash)\"",
    "~/.local/bin/mise activate fish | source",
];

/// Default config.fish written by releases up to v1.0.x.
const LEGACY_FISH_DEFAULTS: &str = "
# colors
export LS_COLORS=\"di=1;36:ln=35:so=32:pi=33:ex=31:bd=34;46:cd=34;43:su=30;41:sg=30;46:tw=30;42:ow=30;43\"

# path
set PATH $PATH ~/.local/bin ~/.local/share/mise/shims

# aliases
alias ls='eza --icons=always'
alias la='ls -a'
alias ll='eza -lah'
alias l='eza -lah --classify --grid'

alias vim='v'
alias v='nvim'
alias vd='nvim -d'
alias cat='BAT_THEME=Dracula bat --paging=never --plain'

function history
    builtin history --show-time=\"%Y-%m-%d %H:%M:%S \" $argv
end

";

/// Shell files devenv may have edited, relative to $HOME.
const SHELL_FILES: [&str; 5] = [
    ".bashrc",
    ".bash_profile",
    ".bash_login",
    ".profile",
    ".config/fish/config.fish",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Rewrite a file without devenv's blocks (backed up first).
    CleanFile { path: PathBuf },
    /// Delete a file devenv created and that holds nothing else.
    DeleteFile { path: PathBuf },
    /// Delete a directory tree (purge only).
    DeleteDir { path: PathBuf, bytes: u64 },
}

impl Action {
    pub fn describe(&self) -> String {
        match self {
            Action::CleanFile { path } => {
                format!("Remove devenv's mise setup from {}", path.display())
            }
            Action::DeleteFile { path } => format!("Delete {}", path.display()),
            Action::DeleteDir { path, bytes } => {
                format!("Delete {} ({})", path.display(), human_size(*bytes))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallPlan {
    pub purge: bool,
    pub actions: Vec<Action>,
    /// Things deliberately left alone, shown to the user.
    pub notes: Vec<String>,
}

impl UninstallPlan {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

/// Where mise keeps its files, following mise's own environment variables.
struct MiseDirs {
    data: PathBuf,
    config: PathBuf,
    cache: PathBuf,
    state: PathBuf,
}

impl MiseDirs {
    fn resolve(home: &Path, var: impl Fn(&str) -> Option<OsString>) -> Self {
        let pick = |mise_var: &str, xdg_var: &str, fallback: &str| {
            let set = |name: &str| var(name).filter(|value| !value.is_empty());
            set(mise_var).map(PathBuf::from).unwrap_or_else(|| {
                set(xdg_var)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| home.join(fallback))
                    .join("mise")
            })
        };
        Self {
            data: pick("MISE_DATA_DIR", "XDG_DATA_HOME", ".local/share"),
            config: pick("MISE_CONFIG_DIR", "XDG_CONFIG_HOME", ".config"),
            cache: pick("MISE_CACHE_DIR", "XDG_CACHE_HOME", ".cache"),
            state: pick("MISE_STATE_DIR", "XDG_STATE_HOME", ".local/state"),
        }
    }
}

/// Work out what an uninstall would do for the current user.
pub fn plan(purge: bool) -> Result<UninstallPlan> {
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is not set"))?;
    plan_in_home(Path::new(&home), purge, |name| std::env::var_os(name))
}

fn plan_in_home(
    home: &Path,
    purge: bool,
    var: impl Fn(&str) -> Option<OsString>,
) -> Result<UninstallPlan> {
    let mut actions = Vec::new();
    for name in SHELL_FILES {
        let path = home.join(name);
        let Some(contents) = read_if_file(&path)? else {
            continue;
        };
        let cleaned = strip_devenv(&contents);
        if name.ends_with("config.fish") && is_default_fish_config(&cleaned) {
            actions.push(Action::DeleteFile { path });
        } else if cleaned != contents {
            actions.push(Action::CleanFile { path });
        }
    }
    let conf_d = home.join(FISH_ACTIVATION_FILE);
    if conf_d.is_file() {
        actions.push(Action::DeleteFile { path: conf_d });
    }

    let mut notes = Vec::new();
    if purge {
        let mise_bin = home.join(".local/bin/mise");
        if mise_bin.symlink_metadata().is_ok() {
            actions.push(Action::DeleteFile { path: mise_bin });
        }
        let dirs = MiseDirs::resolve(home, &var);
        for dir in [dirs.data, dirs.config, dirs.cache, dirs.state] {
            if dir.symlink_metadata().is_ok_and(|meta| meta.is_dir()) {
                let bytes = dir_size(&dir);
                actions.push(Action::DeleteDir { path: dir, bytes });
            }
        }
        if home.join(".rustup").exists() {
            notes.push(
                "Rust toolchains stay in ~/.rustup and ~/.cargo; run `rustup self uninstall` to remove them."
                    .to_string(),
            );
        }
    } else {
        notes.push(
            "mise and the tools it installed stay on disk; uninstall with --purge to remove them."
                .to_string(),
        );
    }
    if home.join(".config/nvim").exists() {
        notes.push(
            "Your Neovim config (~/.config/nvim) is left in place. Earlier configs saved as ~/.config/nvim.bak* can be restored by hand."
                .to_string(),
        );
    }
    notes.push(
        "System packages (Build Tools and prerequisites) are not removed; other software may use them."
            .to_string(),
    );
    Ok(UninstallPlan {
        purge,
        actions,
        notes,
    })
}

/// Carry out a plan. Every action is attempted; each reports its own result.
pub fn execute<F>(plan: &UninstallPlan, mut log: F) -> Vec<(String, Result<(), String>)>
where
    F: FnMut(&str),
{
    plan.actions
        .iter()
        .map(|action| {
            let description = action.describe();
            let result = run_action(action, &mut log).map_err(|error| format!("{error:#}"));
            match &result {
                Ok(()) => log(&format!("Done: {description}")),
                Err(error) => log(&format!("[ERROR] {description}: {error}")),
            }
            (description, result)
        })
        .collect()
}

fn run_action(action: &Action, log: &mut impl FnMut(&str)) -> Result<()> {
    match action {
        Action::CleanFile { path } => {
            let contents = fs::read_to_string(path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let backup = next_backup_path(path)?;
            fs::copy(path, &backup)
                .with_context(|| format!("Failed to back up {}", path.display()))?;
            log(&format!(
                "Backed up {} to {}",
                path.display(),
                backup.display()
            ));
            write_atomically(path, &strip_devenv(&contents))
        }
        Action::DeleteFile { path } => {
            fs::remove_file(path).with_context(|| format!("Failed to delete {}", path.display()))
        }
        Action::DeleteDir { path, .. } => {
            // Refuse to follow a symlink out of the user's mise directories.
            if path.symlink_metadata()?.file_type().is_symlink() {
                return fs::remove_file(path)
                    .with_context(|| format!("Failed to delete {}", path.display()));
            }
            fs::remove_dir_all(path).with_context(|| format!("Failed to delete {}", path.display()))
        }
    }
}

/// Remove marked blocks (and the blank line devenv put before each), legacy
/// unmarked blocks, and legacy one-line activations. Everything else stays.
pub(crate) fn strip_devenv(contents: &str) -> String {
    let mut text = contents.to_string();
    for block in LEGACY_BLOCKS {
        // devenv appended "\n{block}" after the file's own trailing newline.
        text = text
            .replace(&format!("\n\n{block}"), "\n")
            .replace(block, "");
    }

    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut in_block = false;
    for line in lines {
        let bare = line.trim_end_matches(['\n', '\r']);
        if in_block {
            in_block = bare.trim() != BLOCK_END;
            continue;
        }
        if bare.trim() == BLOCK_BEGIN {
            if kept
                .last()
                .is_some_and(|previous| previous.trim().is_empty())
            {
                kept.pop();
            }
            in_block = true;
            continue;
        }
        if LEGACY_LINES.contains(&bare) {
            continue;
        }
        kept.push(line);
    }
    kept.concat()
}

fn is_default_fish_config(contents: &str) -> bool {
    let trimmed = contents.trim();
    trimmed.is_empty() || trimmed == FISH_DEFAULTS.trim() || trimmed == LEGACY_FISH_DEFAULTS.trim()
}

fn read_if_file(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("Failed to read {}", path.display())),
    }
}

fn dir_size(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => dir_size(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |meta| meta.len()),
            _ => 0,
        })
        .sum()
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Home(PathBuf);
    impl Home {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "devenv-uninstall-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn write(&self, name: &str, contents: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, contents).unwrap();
            path
        }
    }
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const BLOCK: &str =
        "# >>> devenv-linux >>>\n# Added by devenv-linux\nexport X=1\n# <<< devenv-linux <<<\n";

    #[test]
    fn strip_should_remove_marked_block_and_its_blank_line_only() {
        let original = format!("# mine\nalias g=git\n\n{BLOCK}# after\n");

        assert_eq!(strip_devenv(&original), "# mine\nalias g=git\n# after\n");
    }

    #[test]
    fn strip_should_remove_legacy_blocks_and_lines() {
        let original = format!(
            "# mine\n\n{}~/.local/bin/mise activate fish | source\n# end\n",
            LEGACY_BLOCKS[0]
        );

        assert_eq!(strip_devenv(&original), "# mine\n# end\n");
    }

    #[test]
    fn strip_should_leave_user_activation_untouched() {
        let original = "eval \"$(/opt/mise activate bash)\"\n";

        assert_eq!(strip_devenv(original), original);
    }

    #[test]
    fn plan_should_clean_shell_files_and_delete_default_fish_config() {
        let home = Home::new();
        let bashrc = home.write(".bashrc", &format!("# mine\n\n{BLOCK}"));
        home.write(".profile", "# untouched\n");
        let fish = home.write(".config/fish/config.fish", FISH_DEFAULTS);
        let conf_d = home.write(FISH_ACTIVATION_FILE, BLOCK);

        let plan = plan_in_home(&home.0, false, |_| None).unwrap();

        assert_eq!(
            plan.actions,
            vec![
                Action::CleanFile {
                    path: bashrc.clone()
                },
                Action::DeleteFile { path: fish },
                Action::DeleteFile { path: conf_d },
            ]
        );
        assert!(plan.notes.iter().any(|note| note.contains("--purge")));

        let results = execute(&plan, |_| {});
        assert!(results.iter().all(|(_, result)| result.is_ok()));
        assert_eq!(fs::read_to_string(&bashrc).unwrap(), "# mine\n");
        assert!(home.0.join(".bashrc.bak").exists());
        assert!(!home.0.join(".config/fish/config.fish").exists());
        assert!(plan_in_home(&home.0, false, |_| None).unwrap().is_empty());
    }

    #[test]
    fn plan_should_keep_customized_fish_config() {
        let home = Home::new();
        let fish = home.write(
            ".config/fish/config.fish",
            &format!("{FISH_DEFAULTS}set -g mine 1\n"),
        );

        let plan = plan_in_home(&home.0, false, |_| None).unwrap();

        assert!(plan.is_empty(), "{plan:?}");
        assert!(fish.exists());
    }

    #[test]
    fn purge_should_remove_mise_dirs_but_never_nvim() {
        let home = Home::new();
        home.write(".local/bin/mise", "bin");
        home.write(".local/share/mise/installs/node/bin/node", "node");
        home.write(".config/mise/config.toml", "[tools]\n");
        home.write(".config/nvim/init.lua", "-- mine\n");
        let custom_cache = home.0.join("custom-cache");
        home.write("custom-cache/file", "x");

        let plan = plan_in_home(&home.0, true, |name| {
            (name == "MISE_CACHE_DIR").then(|| custom_cache.clone().into_os_string())
        })
        .unwrap();

        let paths: Vec<_> = plan
            .actions
            .iter()
            .map(|action| match action {
                Action::DeleteFile { path } | Action::DeleteDir { path, .. } => path.clone(),
                Action::CleanFile { path } => path.clone(),
            })
            .collect();
        assert_eq!(
            paths,
            vec![
                home.0.join(".local/bin/mise"),
                home.0.join(".local/share/mise"),
                home.0.join(".config/mise"),
                custom_cache,
            ]
        );
        assert!(plan.notes.iter().any(|note| note.contains("nvim.bak")));

        execute(&plan, |_| {});
        assert!(!home.0.join(".local/share/mise").exists());
        assert!(home.0.join(".config/nvim/init.lua").exists());
    }

    #[test]
    fn purge_should_delete_a_symlinked_dir_link_not_its_target() {
        let home = Home::new();
        let target = home.0.join("elsewhere");
        home.write("elsewhere/keep", "x");
        fs::create_dir_all(home.0.join(".local/share")).unwrap();
        std::os::unix::fs::symlink(&target, home.0.join(".local/share/mise")).unwrap();

        let action = Action::DeleteDir {
            path: home.0.join(".local/share/mise"),
            bytes: 0,
        };
        run_action(&action, &mut |_| {}).unwrap();

        assert!(target.join("keep").exists());
    }

    #[test]
    fn human_size_should_scale_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(3 * 1024 * 1024), "3.0 MB");
    }
}
