use crate::registry::Component;
use crate::sys::{CommandResult, run_cmd_streaming};
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigOutcome {
    Changed,
    AlreadyConfigured,
}

pub fn setup_config<F>(component: &Component, log: F) -> Result<ConfigOutcome>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    setup_config_in_home(component, Path::new(&home), log)
}

fn setup_config_in_home<F>(component: &Component, home: &Path, mut log: F) -> Result<ConfigOutcome>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    match component.id.as_str() {
        "config-bash" => setup_shell_path_in_home(home, false, log),
        "config-fish" => setup_fish_config(home, log),
        "config-nvim" => {
            let nvim_dir = home.join(".config/nvim");
            let staging_dir = next_staging_path(&nvim_dir)?;
            let Some(parent) = staging_dir.parent() else {
                return Err(anyhow!("nvim staging path has no parent"));
            };
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create nvim config parent {}", parent.display())
            })?;

            let preparation = (|| -> Result<()> {
                let staging = staging_dir
                    .to_str()
                    .ok_or_else(|| anyhow!("nvim staging path is not valid UTF-8"))?;
                log("Cloning LazyVim starter into staging...");
                let result = run_cmd_streaming(
                    "git",
                    &["clone", "https://github.com/LazyVim/starter", staging],
                    log.clone(),
                )?;
                ensure_success(result, "clone LazyVim starter")?;
                if let Ok(output) = std::process::Command::new("git")
                    .args(["-C", staging, "rev-parse", "HEAD"])
                    .output()
                    && output.status.success()
                {
                    log(&format!(
                        "Using LazyVim starter commit {}",
                        String::from_utf8_lossy(&output.stdout).trim()
                    ));
                }

                fs::remove_dir_all(staging_dir.join(".git"))
                    .context("Failed to remove LazyVim git metadata")?;

                log("Appending OSC52 clipboard configuration...");
                let osc52_cfg = "

-- OSC 52 clipboard configuration
vim.opt.clipboard = \"unnamedplus\"

vim.g.clipboard = {
  name = \"OSC 52\",
  copy = {
    [\"+\"] = require(\"vim.ui.clipboard.osc52\").copy(\"+\"),
    [\"*\"] = require(\"vim.ui.clipboard.osc52\").copy(\"*\"),
  },
  paste = {
    [\"+\"] = require(\"vim.ui.clipboard.osc52\").paste(\"+\"),
    [\"*\"] = require(\"vim.ui.clipboard.osc52\").paste(\"*\"),
  },
}
";
                use std::io::Write;
                let opt_file = staging_dir.join("lua/config/options.lua");
                let mut file = fs::OpenOptions::new()
                    .append(true)
                    .open(&opt_file)
                    .with_context(|| format!("Failed to open {}", opt_file.display()))?;
                write!(file, "{osc52_cfg}").with_context(|| {
                    format!("Failed to append OSC52 config to {}", opt_file.display())
                })?;
                Ok(())
            })();

            if let Err(error) = preparation {
                let _ = fs::remove_dir_all(&staging_dir);
                return Err(error);
            }

            let backup = swap_staged_config(&staging_dir, &nvim_dir)?;
            if let Some(backup) = backup {
                log(&format!(
                    "Existing nvim config backed up to {}",
                    backup.display()
                ));
            }
            Ok(ConfigOutcome::Changed)
        }
        _ => {
            log(&format!("Unknown config component: {}", component.id));
            Ok(ConfigOutcome::Changed)
        }
    }
}

const FISH_DEFAULTS: &str = r#"
# path
fish_add_path --append ~/.local/bin ~/.local/share/mise/shims

if status is-interactive
    # colors
    set -gx LS_COLORS "di=1;36:ln=35:so=32:pi=33:ex=31:bd=34;46:cd=34;43:su=30;41:sg=30;46:tw=30;42:ow=30;43"

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
        builtin history --show-time="%Y-%m-%d %H:%M:%S " $argv
    end
end

"#;

/// Every block devenv adds to a user file starts with this marker and ends with
/// "# <<< devenv-linux <<<", so it can be found again (to skip re-adding it,
/// or to remove it on uninstall).
const BLOCK_BEGIN: &str = "# >>> devenv-linux >>>";

/// Appended to ~/.bashrc: interactive bash activates mise.
const BASH_ACTIVATION: &str = r#"# >>> devenv-linux >>>
# Added by devenv-linux: activates mise in interactive bash so mise and the
# tools it manages are on PATH. Remove this block to undo.
if ! command -v mise >/dev/null 2>&1; then
    export PATH="$HOME/.local/bin:$PATH"
fi
case $- in
    *i*) command -v mise >/dev/null 2>&1 && eval "$(mise activate bash)" ;;
esac
# <<< devenv-linux <<<
"#;

/// Appended to the login profile: login shells, scripts, and SSH commands get
/// mise and the shims of its tools on PATH without an interactive hook.
const PROFILE_PATH: &str = r#"# >>> devenv-linux >>>
# Added by devenv-linux: puts mise and the shims of the tools it manages on
# PATH for login shells, scripts, and SSH commands. Interactive bash also
# activates mise from ~/.bashrc. Remove this block to undo.
devenv_shims="${MISE_DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/mise}/shims"
for devenv_dir in "$devenv_shims" "$HOME/.local/bin"; do
    case ":$PATH:" in
        *":$devenv_dir:"*) ;;
        *) PATH="$devenv_dir:$PATH" ;;
    esac
done
export PATH
unset devenv_dir devenv_shims
# <<< devenv-linux <<<
"#;

/// Written to ~/.config/fish/conf.d/devenv-mise.fish, which fish loads on
/// start, so fish sees mise tools without touching config.fish.
const FISH_ACTIVATION: &str = r#"# >>> devenv-linux >>>
# Added by devenv-linux: activates mise in fish so mise and the tools it
# manages are on PATH. Delete this file to undo.
if not type -q mise
    set -gx PATH "$HOME/.local/bin" $PATH
end
if type -q mise
    if status is-interactive
        mise activate fish | source
    else
        mise activate fish --shims | source
    end
end
# <<< devenv-linux <<<
"#;

pub(crate) const FISH_ACTIVATION_FILE: &str = ".config/fish/conf.d/devenv-mise.fish";

pub(crate) fn has_shell_activation(contents: &str, shell: &str) -> bool {
    contents.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#') && line.contains(&format!("mise activate {shell}"))
    })
}

fn has_block(contents: &str) -> bool {
    contents.lines().any(|line| line.trim() == BLOCK_BEGIN)
}

/// The file bash reads for login shells: the first of ~/.bash_profile,
/// ~/.bash_login, and ~/.profile that exists, or ~/.profile.
pub(crate) fn login_profile(home: &Path) -> PathBuf {
    [".bash_profile", ".bash_login"]
        .iter()
        .map(|name| home.join(name))
        .find(|path| path.exists())
        .unwrap_or_else(|| home.join(".profile"))
}

/// Make mise and its tools reachable from the user's shells: activation in
/// ~/.bashrc, a PATH block in the login profile, and (for fish users) a
/// conf.d activation file. Every step is idempotent.
pub fn setup_shell_path<F>(fish: bool, log: F) -> Result<ConfigOutcome>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    setup_shell_path_in_home(Path::new(&home), fish, log)
}

fn setup_shell_path_in_home<F>(home: &Path, fish: bool, log: F) -> Result<ConfigOutcome>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    let mut outcomes = vec![
        update_shell_config(
            &home.join(".bashrc"),
            "bash",
            BASH_ACTIVATION,
            "",
            log.clone(),
        )?,
        append_block(&login_profile(home), PROFILE_PATH, log.clone())?,
    ];
    if fish {
        outcomes.push(ensure_fish_activation(home, log)?);
    }
    Ok(combine(&outcomes))
}

/// Fish defaults (aliases, colors) go to config.fish only when it does not
/// exist yet; activation lives in conf.d. Legacy installer lines are migrated.
fn setup_fish_config<F>(home: &Path, mut log: F) -> Result<ConfigOutcome>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    let config = home.join(".config/fish/config.fish");
    let mut outcomes = Vec::new();
    match fs::read_to_string(&config) {
        Ok(contents) => {
            if contents.lines().any(|line| line == LEGACY_FISH_ACTIVATION) {
                outcomes.push(update_shell_config(
                    &config,
                    "fish",
                    FISH_ACTIVATION,
                    "",
                    log.clone(),
                )?);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = config.parent() {
                fs::create_dir_all(parent)?;
            }
            write_atomically(&config, FISH_DEFAULTS)?;
            log(&format!(
                "Wrote default fish configuration to {}",
                config.display()
            ));
            outcomes.push(ConfigOutcome::Changed);
        }
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", config.display()));
        }
    }
    outcomes.push(ensure_fish_activation(home, log)?);
    Ok(combine(&outcomes))
}

fn ensure_fish_activation<F>(home: &Path, mut log: F) -> Result<ConfigOutcome>
where
    F: FnMut(&str),
{
    let config = fs::read_to_string(home.join(".config/fish/config.fish")).unwrap_or_default();
    let file = home.join(FISH_ACTIVATION_FILE);
    if has_shell_activation(&config, "fish") || file.exists() {
        return Ok(ConfigOutcome::AlreadyConfigured);
    }
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    write_atomically(&file, FISH_ACTIVATION)?;
    log(&format!("Wrote fish mise activation to {}", file.display()));
    Ok(ConfigOutcome::Changed)
}

/// Append a marked block to a file unless one is already there, backing the
/// file up first.
fn append_block<F>(destination: &Path, block: &str, mut log: F) -> Result<ConfigOutcome>
where
    F: FnMut(&str),
{
    let existing = match fs::read_to_string(destination) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", destination.display()));
        }
    };
    if existing.as_deref().is_some_and(has_block) {
        return Ok(ConfigOutcome::AlreadyConfigured);
    }
    let updated = match &existing {
        Some(contents) => format!("{contents}\n{block}"),
        None => block.to_string(),
    };
    if existing.is_some() {
        let backup = next_backup_path(destination)?;
        fs::copy(destination, &backup)
            .with_context(|| format!("Failed to back up {}", destination.display()))?;
        log(&format!(
            "Existing configuration backed up to {}",
            backup.display()
        ));
    }
    write_atomically(destination, &updated)?;
    log(&format!(
        "Added mise PATH setup to {}",
        destination.display()
    ));
    Ok(ConfigOutcome::Changed)
}

fn combine(outcomes: &[ConfigOutcome]) -> ConfigOutcome {
    if outcomes.contains(&ConfigOutcome::Changed) {
        ConfigOutcome::Changed
    } else {
        ConfigOutcome::AlreadyConfigured
    }
}

const LEGACY_FISH_ACTIVATION: &str = "~/.local/bin/mise activate fish | source";

fn update_shell_config<F>(
    destination: &Path,
    shell: &str,
    activation: &str,
    defaults: &str,
    mut log: F,
) -> Result<ConfigOutcome>
where
    F: FnMut(&str),
{
    let existing = match fs::read_to_string(destination) {
        Ok(contents) => Some(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to read {}", destination.display()));
        }
    };
    let original = existing.as_deref().unwrap_or(defaults);
    let legacy = match shell {
        "fish" => LEGACY_FISH_ACTIVATION,
        _ => "eval \"$($HOME/.local/bin/mise activate bash)\"",
    };
    let has_legacy = original.lines().any(|line| line == legacy);
    if !has_legacy && has_shell_activation(original, shell) {
        return Ok(ConfigOutcome::AlreadyConfigured);
    }
    let updated = if has_legacy {
        original
            .split_inclusive('\n')
            .map(|line| {
                if line.trim_end_matches('\n') == legacy {
                    activation
                } else {
                    line
                }
            })
            .collect::<String>()
    } else {
        format!("{original}\n{activation}")
    };
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if existing.is_some() {
        let backup = next_backup_path(destination)?;
        fs::copy(destination, &backup)
            .with_context(|| format!("Failed to back up {}", destination.display()))?;
        log(&format!(
            "Existing configuration backed up to {}",
            backup.display()
        ));
    }
    write_atomically(destination, &updated)?;
    Ok(ConfigOutcome::Changed)
}

/// Replace a file's contents via a temporary sibling and rename, so a crash
/// or full disk never leaves a truncated shell config. Symlinks (e.g. from a
/// dotfile manager) are resolved so the link itself is preserved, and the
/// original file permissions are kept.
fn write_atomically(destination: &Path, contents: &str) -> Result<()> {
    let target = if destination.is_symlink() {
        fs::canonicalize(destination)
            .with_context(|| format!("Failed to resolve symlink {}", destination.display()))?
    } else {
        destination.to_path_buf()
    };
    let temporary = next_numbered_path(&target, ".devenv-tmp")?;

    let result = (|| -> Result<()> {
        use std::io::Write;
        let mut file = fs::File::create(&temporary)
            .with_context(|| format!("Failed to create {}", temporary.display()))?;
        file.write_all(contents.as_bytes())
            .and_then(|()| file.sync_all())
            .with_context(|| format!("Failed to write {}", temporary.display()))?;
        if let Ok(metadata) = fs::metadata(&target) {
            fs::set_permissions(&temporary, metadata.permissions()).with_context(|| {
                format!("Failed to copy permissions to {}", temporary.display())
            })?;
        }
        fs::rename(&temporary, &target)
            .with_context(|| format!("Failed to replace {}", target.display()))
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn next_staging_path(destination: &Path) -> Result<PathBuf> {
    next_numbered_path(destination, ".devenv-staging")
}

fn next_backup_path(destination: &Path) -> Result<PathBuf> {
    next_numbered_path(destination, ".bak")
}

fn next_numbered_path(destination: &Path, suffix: &str) -> Result<PathBuf> {
    let mut base = destination.as_os_str().to_os_string();
    base.push(suffix);
    let base = PathBuf::from(base);
    if !base.exists() {
        return Ok(base);
    }

    for index in 1_u32..=u32::MAX {
        let mut candidate = base.as_os_str().to_os_string();
        candidate.push(format!(".{index}"));
        let candidate = PathBuf::from(candidate);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(anyhow!(
        "No available numbered path after {}",
        base.display()
    ))
}

fn swap_staged_config(staging: &Path, destination: &Path) -> Result<Option<PathBuf>> {
    let backup = if destination.exists() {
        let backup = next_backup_path(destination)?;
        fs::rename(destination, &backup).with_context(|| {
            format!(
                "Failed to back up nvim config from {} to {}",
                destination.display(),
                backup.display()
            )
        })?;
        Some(backup)
    } else {
        None
    };

    if let Err(swap_error) = fs::rename(staging, destination) {
        if let Some(backup) = &backup
            && let Err(restore_error) = fs::rename(backup, destination)
        {
            return Err(anyhow!(
                "Failed to install staged nvim config: {swap_error}; also failed to restore original: {restore_error}"
            ));
        }
        let _ = fs::remove_dir_all(staging);
        return Err(anyhow!(
            "Failed to install staged nvim config: {swap_error}"
        ));
    }

    Ok(backup)
}

fn ensure_success(result: CommandResult, action: &str) -> Result<()> {
    if result.success {
        Ok(())
    } else {
        let stderr = result.stderr.trim();
        if stderr.is_empty() {
            Err(anyhow!("Failed to {action}"))
        } else {
            Err(anyhow!("Failed to {action}: {stderr}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Category, Group};
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn legacy_activation_should_be_backed_up_migrated_and_idempotent() {
        for (shell, legacy, activation) in [
            (
                "fish",
                "~/.local/bin/mise activate fish | source",
                FISH_ACTIVATION,
            ),
            (
                "bash",
                "eval \"$($HOME/.local/bin/mise activate bash)\"",
                BASH_ACTIVATION,
            ),
        ] {
            let root = TestDir::new();
            let dest = root.path().join(shell);
            let original = format!("# user settings\n{legacy}\n# keep me\n");
            fs::write(&dest, &original).unwrap();
            update_shell_config(&dest, shell, activation, "", |_| {}).unwrap();
            assert_eq!(
                fs::read_to_string(root.path().join(format!("{shell}.bak"))).unwrap(),
                original
            );
            let updated = fs::read_to_string(&dest).unwrap();
            assert!(updated.contains(activation) && updated.ends_with("# keep me\n"));
            assert_eq!(
                update_shell_config(&dest, shell, activation, "", |_| {}).unwrap(),
                ConfigOutcome::AlreadyConfigured
            );
        }
    }

    #[test]
    fn commented_activation_should_not_prevent_setup() {
        let root = TestDir::new();
        let dest = root.path().join("config.fish");
        fs::write(&dest, "# mise activate fish | source\n").unwrap();
        assert_eq!(
            update_shell_config(&dest, "fish", FISH_ACTIVATION, "", |_| {}).unwrap(),
            ConfigOutcome::Changed
        );
    }

    #[test]
    fn custom_activation_should_be_preserved() {
        let root = TestDir::new();
        let dest = root.path().join("config.fish");
        let original = "if status is-interactive\n    /opt/mise activate fish | source\nend\n";
        fs::write(&dest, original).unwrap();
        assert_eq!(
            update_shell_config(&dest, "fish", FISH_ACTIVATION, "", |_| {}).unwrap(),
            ConfigOutcome::AlreadyConfigured
        );
        assert_eq!(fs::read_to_string(dest).unwrap(), original);
    }

    #[test]
    fn atomic_write_should_preserve_symlinks_and_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let root = TestDir::new();
        let target = root.path().join("dotfiles-bashrc");
        let link = root.path().join(".bashrc");
        fs::write(&target, "# managed elsewhere\n").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&target, &link).unwrap();

        update_shell_config(&link, "bash", BASH_ACTIVATION, "", |_| {}).unwrap();

        assert!(link.is_symlink());
        assert!(
            fs::read_to_string(&target)
                .unwrap()
                .contains(BASH_ACTIVATION)
        );
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(fs::read_dir(root.path()).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("devenv-tmp")
        }));
    }

    #[test]
    fn shell_path_should_use_existing_bash_profile_and_be_idempotent() {
        let root = TestDir::new();
        fs::write(root.path().join(".bash_profile"), "# fedora default\n").unwrap();

        let first = setup_shell_path_in_home(root.path(), false, |_| {}).unwrap();
        let second = setup_shell_path_in_home(root.path(), false, |_| {}).unwrap();

        let profile = fs::read_to_string(root.path().join(".bash_profile")).unwrap();
        assert_eq!(
            (first, second),
            (ConfigOutcome::Changed, ConfigOutcome::AlreadyConfigured)
        );
        assert!(profile.starts_with("# fedora default\n") && profile.contains(PROFILE_PATH));
        assert_eq!(profile.matches(BLOCK_BEGIN).count(), 1);
        assert!(!root.path().join(".profile").exists());
        assert!(root.path().join(".bash_profile.bak").exists());
    }

    #[test]
    fn profile_block_should_prepend_mise_dirs_once() {
        let root = TestDir::new();
        let profile = root.path().join(".profile");
        fs::write(&profile, PROFILE_PATH).unwrap();

        let output = std::process::Command::new("sh")
            .args([
                "-c",
                ". \"$HOME/.profile\"; . \"$HOME/.profile\"; echo \"$PATH\"",
            ])
            .env("HOME", root.path())
            .env("PATH", "/usr/bin:/bin")
            .env_remove("MISE_DATA_DIR")
            .env_remove("XDG_DATA_HOME")
            .output()
            .unwrap();

        let home = root.path().display();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            format!("{home}/.local/bin:{home}/.local/share/mise/shims:/usr/bin:/bin")
        );
    }

    #[test]
    fn fish_activation_should_use_conf_d_unless_config_fish_activates() {
        let root = TestDir::new();
        assert_eq!(
            setup_shell_path_in_home(root.path(), true, |_| {}).unwrap(),
            ConfigOutcome::Changed
        );
        assert!(root.path().join(FISH_ACTIVATION_FILE).exists());

        let custom = TestDir::new();
        let config = custom.path().join(".config/fish/config.fish");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "mise activate fish | source\n").unwrap();
        ensure_fish_activation(custom.path(), |_| {}).unwrap();
        assert!(!custom.path().join(FISH_ACTIVATION_FILE).exists());
    }

    #[test]
    fn fish_config_should_not_overwrite_existing_config_fish() {
        let root = TestDir::new();
        let config = root.path().join(".config/fish/config.fish");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "set -g my_setting 1\n").unwrap();

        setup_fish_config(root.path(), |_| {}).unwrap();

        assert_eq!(
            fs::read_to_string(&config).unwrap(),
            "set -g my_setting 1\n"
        );
        assert!(root.path().join(FISH_ACTIVATION_FILE).exists());
    }

    struct TestDir(PathBuf);

    impl TestDir {
        fn new() -> Self {
            let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "devenv-config-test-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("test directory should be created");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn config_component(id: &str) -> Component {
        Component::new(
            id,
            id,
            "test configuration",
            Category::Config,
            Group::Configurations,
            None,
            &[],
        )
    }

    #[test]
    fn backup_path_should_use_numbered_suffixes_without_overwriting() {
        let root = TestDir::new();
        let destination = root.path().join("nvim");
        fs::create_dir(&destination).expect("destination should be created");
        fs::create_dir(root.path().join("nvim.bak")).expect("first backup should be created");
        fs::create_dir(root.path().join("nvim.bak.1")).expect("second backup should be created");

        assert_eq!(
            next_backup_path(&destination).expect("backup path should be available"),
            root.path().join("nvim.bak.2")
        );
    }

    #[test]
    fn staged_swap_should_back_up_existing_config_after_staging_succeeds() {
        let root = TestDir::new();
        let destination = root.path().join("nvim");
        let staging = root.path().join("nvim.devenv-staging");
        fs::create_dir(&destination).expect("destination should be created");
        fs::write(destination.join("marker"), "old").expect("old marker should be written");
        fs::create_dir(&staging).expect("staging should be created");
        fs::write(staging.join("marker"), "new").expect("new marker should be written");

        let backup = swap_staged_config(&staging, &destination)
            .expect("staged config should be swapped")
            .expect("existing config should be backed up");

        assert_eq!(
            (
                fs::read_to_string(destination.join("marker")).expect("new marker should exist"),
                fs::read_to_string(backup.join("marker")).expect("old marker should exist"),
            ),
            ("new".to_string(), "old".to_string())
        );
    }

    #[test]
    fn staged_swap_should_restore_original_when_final_swap_fails() {
        let root = TestDir::new();
        let destination = root.path().join("nvim");
        let missing_staging = root.path().join("missing-staging");
        fs::create_dir(&destination).expect("destination should be created");
        fs::write(destination.join("marker"), "old").expect("old marker should be written");

        let error = swap_staged_config(&missing_staging, &destination)
            .expect_err("missing staging should fail");

        assert!(
            error
                .to_string()
                .contains("Failed to install staged nvim config")
                && destination.join("marker").exists()
                && !root.path().join("nvim.bak").exists()
        );
    }

    #[test]
    fn bash_configuration_should_be_idempotent() {
        let root = TestDir::new();
        let component = config_component("config-bash");

        let first = setup_config_in_home(&component, root.path(), |_| {})
            .expect("first setup should succeed");
        let second = setup_config_in_home(&component, root.path(), |_| {})
            .expect("second setup should succeed");

        assert_eq!(
            (first, second),
            (ConfigOutcome::Changed, ConfigOutcome::AlreadyConfigured)
        );
    }

    #[test]
    fn fish_configuration_should_be_idempotent() {
        let root = TestDir::new();
        let component = config_component("config-fish");

        let first = setup_config_in_home(&component, root.path(), |_| {})
            .expect("first setup should succeed");
        let second = setup_config_in_home(&component, root.path(), |_| {})
            .expect("second setup should succeed");

        assert_eq!(
            (first, second),
            (ConfigOutcome::Changed, ConfigOutcome::AlreadyConfigured)
        );
    }
}
