use crate::registry::Component;
use crate::sys::{CommandResult, run_cmd, run_cmd_streaming};
use anyhow::{Context, Result, anyhow};
use std::fs;
use std::path::Path;

pub fn setup_config<F>(component: &Component, mut log: F) -> Result<()>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());

    match component.id.as_str() {
        "config-bash" => {
            let bashrc = format!("{}/.bashrc", home);
            let mise_line = "eval \"$($HOME/.local/bin/mise activate bash)\"\n";

            let already_set = if Path::new(&bashrc).exists() {
                fs::read_to_string(&bashrc)
                    .map(|s| s.contains("mise activate bash"))
                    .unwrap_or(false)
            } else {
                false
            };

            if already_set {
                log("Bash .bashrc already contains mise activation -- skipping.");
            } else {
                log(&format!("Appending mise activation to {}", bashrc));
                use std::io::Write;
                let mut f = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&bashrc)
                    .with_context(|| format!("Failed to open {bashrc}"))?;
                writeln!(f, "\n# mise activation -- added by devenv-linux installer")
                    .with_context(|| format!("Failed to update {bashrc}"))?;
                write!(f, "{}", mise_line).with_context(|| format!("Failed to write {bashrc}"))?;
            }
            Ok(())
        }
        "config-fish" => {
            let config_dir = format!("{}/.config/fish", home);
            fs::create_dir_all(&config_dir)
                .with_context(|| format!("Failed to create {config_dir}"))?;
            let dest = format!("{}/config.fish", config_dir);

            let already_set = if Path::new(&dest).exists() {
                fs::read_to_string(&dest)
                    .map(|s| s.contains("mise activate fish"))
                    .unwrap_or(false)
            } else {
                false
            };

            if already_set {
                log("Fish config.fish already contains mise activation -- skipping.");
                return Ok(());
            }

            let mise_line = "~/.local/bin/mise activate fish | source\n";

            if !Path::new(&dest).exists() {
                log("Writing generic fish config...");
                let content = format!(
                    "
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

# mise activation
{}",
                    mise_line
                );
                fs::write(&dest, content).with_context(|| format!("Failed to write {dest}"))?;
            } else {
                log(&format!("Appending mise activation to {}", dest));
                use std::io::Write;
                let mut f = fs::OpenOptions::new()
                    .append(true)
                    .open(&dest)
                    .with_context(|| format!("Failed to open {dest}"))?;
                writeln!(f, "\n# mise activation -- added by devenv-linux installer")
                    .with_context(|| format!("Failed to update {dest}"))?;
                write!(f, "{}", mise_line).with_context(|| format!("Failed to write {dest}"))?;
            }

            Ok(())
        }
        "config-nvim" => {
            let nvim_dir = format!("{}/.config/nvim", home);
            if Path::new(&nvim_dir).exists() {
                let backup = format!("{}.bak", nvim_dir);
                log(&format!("Backing up existing nvim config to {}", backup));
                run_checked(
                    "mv",
                    &[&nvim_dir, &backup],
                    &format!("back up nvim config to {backup}"),
                )?;
            }

            log("Cloning LazyVim starter...");
            let res = run_cmd_streaming(
                "git",
                &["clone", "https://github.com/LazyVim/starter", &nvim_dir],
                log.clone(),
            )?;
            ensure_success(res, "clone LazyVim starter")?;

            let git_dir = format!("{}/.git", nvim_dir);
            run_checked("rm", &["-rf", &git_dir], "remove LazyVim git metadata")?;

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
            let opt_file = format!("{}/lua/config/options.lua", nvim_dir);
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(&opt_file)
                .with_context(|| format!("Failed to open {opt_file}"))?;
            write!(f, "{}", osc52_cfg)
                .with_context(|| format!("Failed to append OSC52 config to {opt_file}"))?;

            Ok(())
        }
        _ => {
            log(&format!("Unknown config component: {}", component.id));
            Ok(())
        }
    }
}

fn run_checked(cmd: &str, args: &[&str], action: &str) -> Result<()> {
    let result = run_cmd(cmd, args)?;
    ensure_success(result, action)
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
