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
        "config-bash" => {
            update_shell_config(&home.join(".bashrc"), "bash", BASH_ACTIVATION, "", log)
        }
        "config-fish" => update_shell_config(
            &home.join(".config/fish/config.fish"),
            "fish",
            FISH_ACTIVATION,
            FISH_DEFAULTS,
            log,
        ),
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

const FISH_DEFAULTS: &str = "
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

const BASH_ACTIVATION: &str = r#"# mise activation -- added by devenv-linux installer
if ! command -v mise >/dev/null 2>&1; then
    export PATH="$HOME/.local/bin:$PATH"
fi
case $- in
    *i*) eval "$(mise activate bash)" ;;
esac
"#;

const FISH_ACTIVATION: &str = r#"# mise activation -- added by devenv-linux installer
if not type -q mise
    set -gx PATH "$HOME/.local/bin" $PATH
end
if status is-interactive
    mise activate fish | source
else
    mise activate fish --shims | source
end
"#;

pub(crate) fn has_shell_activation(contents: &str, shell: &str) -> bool {
    contents.lines().any(|line| {
        let line = line.trim();
        !line.starts_with('#') && line.contains(&format!("mise activate {shell}"))
    })
}

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
        "fish" => "~/.local/bin/mise activate fish | source",
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
    fs::write(destination, updated)
        .with_context(|| format!("Failed to write {}", destination.display()))?;
    Ok(ConfigOutcome::Changed)
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
