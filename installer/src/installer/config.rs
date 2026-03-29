use crate::registry::{Component, SelectionState};
use crate::sys::{run_cmd, run_cmd_streaming};
use std::fs;
use std::path::Path;

pub fn setup_config<F>(component: &Component, mut log: F) -> Result<(), String>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    if component.state == SelectionState::KeepAsIs {
        log(&format!(
            "Skipping config {} (Keep as-is selected)",
            component.id
        ));
        return Ok(());
    }

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
                log("Bash .bashrc already contains mise activation — skipping.");
            } else {
                log(&format!("Appending mise activation to {}", bashrc));
                use std::io::Write;
                let mut f = fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&bashrc)
                    .map_err(|e| format!("Failed to open {}: {}", bashrc, e))?;
                writeln!(f, "\n# mise activation — added by devenv-linux installer").ok();
                write!(f, "{}", mise_line)
                    .map_err(|e| format!("Failed to write .bashrc: {}", e))?;
            }
            Ok(())
        }

        "config-fish" => {
            let config_dir = format!("{}/.config/fish", home);
            fs::create_dir_all(&config_dir)
                .map_err(|e| format!("Failed to create {}: {}", config_dir, e))?;
            let dest = format!("{}/config.fish", config_dir);

            // If the file exists and has mise activation, skip rewriting it.
            let already_set = if Path::new(&dest).exists() {
                fs::read_to_string(&dest)
                    .map(|s| s.contains("mise activate fish"))
                    .unwrap_or(false)
            } else {
                false
            };

            if already_set {
                log("Fish config.fish already contains mise activation — skipping.");
                return Ok(());
            }

            let mise_line = "~/.local/bin/mise activate fish | source\n";

            // If file doesn't exist, create it with the generic config, plus mise line.
            // If it exists but doesn't have mise, just append mise line.
            if !Path::new(&dest).exists() {
                log("Writing generic fish config...");
                let content = format!("
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
{}
", mise_line);
                fs::write(&dest, content)
                    .map_err(|e| format!("Failed to write config.fish: {}", e))?;
            } else {
                log(&format!("Appending mise activation to {}", dest));
                use std::io::Write;
                let mut f = fs::OpenOptions::new()
                    .append(true)
                    .open(&dest)
                    .map_err(|e| format!("Failed to open {}: {}", dest, e))?;
                writeln!(f, "\n# mise activation — added by devenv-linux installer").ok();
                write!(f, "{}", mise_line)
                    .map_err(|e| format!("Failed to write config.fish: {}", e))?;
            }

            Ok(())
        }

        "config-tmux" => {
            let tmux_dir = format!("{}/.config/tmux", home);
            if Path::new(&tmux_dir).exists() {
                let backup = format!("{}_bk", tmux_dir);
                log(&format!("Backing up existing tmux config to {}", backup));
                let _ = run_cmd("mv", &[&tmux_dir, &backup]);
            }

            log("Cloning oh-my-tmux...");
            let res = run_cmd_streaming(
                "git",
                &[
                    "clone",
                    "--single-branch",
                    "https://github.com/gpakosz/.tmux.git",
                    &tmux_dir,
                ],
                log.clone(),
            );
            if !res.success {
                return Err("Failed to clone oh-my-tmux".into());
            }

            log("Setting up symlinks...");
            let _ = run_cmd(
                "ln",
                &[
                    "-sf",
                    &format!("{}/.tmux.conf", tmux_dir),
                    &format!("{}/.tmux.conf", home),
                ],
            );
            let _ = run_cmd(
                "cp",
                &[
                    &format!("{}/.tmux.conf.local", tmux_dir),
                    &format!("{}/tmux.conf.local", tmux_dir),
                ],
            );

            Ok(())
        }

        "config-nvim" => {
            let nvim_dir = format!("{}/.config/nvim", home);
            if Path::new(&nvim_dir).exists() {
                let backup = format!("{}.bak", nvim_dir);
                log(&format!("Backing up existing nvim config to {}", backup));
                let _ = run_cmd("mv", &[&nvim_dir, &backup]);
            }

            log("Cloning LazyVim starter...");
            let res = run_cmd_streaming(
                "git",
                &["clone", "https://github.com/LazyVim/starter", &nvim_dir],
                log.clone(),
            );
            if !res.success {
                return Err("Failed to clone LazyVim".into());
            }

            let _ = run_cmd("rm", &["-rf", &format!("{}/.git", nvim_dir)]);

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
            if let Ok(mut f) = fs::OpenOptions::new().append(true).open(&opt_file) {
                let _ = write!(f, "{}", osc52_cfg);
            }

            Ok(())
        }

        _ => {
            log(&format!("Unknown config component: {}", component.id));
            Ok(())
        }
    }
}
