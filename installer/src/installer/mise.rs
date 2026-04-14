use crate::registry::{Category, Component};
use crate::sys::run_cmd_streaming;
use anyhow::{anyhow, Result};
use std::fs;

pub fn install_mise<F>(mut log: F) -> Result<()>
where
    F: FnMut(&str) + Send + 'static,
{
    log("Checking for mise...");
    if crate::sys::check_command_exists("mise") {
        log("mise is already installed.");
        return Ok(());
    }

    log("Installing mise...");
    let result = run_cmd_streaming("sh", &["-c", "curl https://mise.run | sh"], log)?;
    if result.success {
        Ok(())
    } else {
        Err(anyhow!("Failed to install mise: {}", result.stderr.trim()))
    }
}

pub fn mise_bin() -> String {
    if crate::sys::check_command_exists("mise") {
        return "mise".to_string();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let home_mise = format!("{}/.local/bin/mise", home);
    if fs::metadata(&home_mise).is_ok() {
        return home_mise;
    }
    "mise".to_string()
}

pub fn activate_mise_tools<F>(components: &[&Component], mut log: F) -> Result<()>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    if components.is_empty() {
        log("No mise components to install.");
        return Ok(());
    }

    let mise = mise_bin();
    let mut failed_plugins = Vec::new();

    for c in components {
        if let Category::Mise(ref plugin) = c.category {
            let tool_spec = format!("{}@latest", plugin);
            log(&format!("Installing: mise use -g {}", tool_spec));

            let result = run_cmd_streaming(&mise, &["use", "-g", &tool_spec], log.clone())?;

            if !result.success {
                let error = result.stderr.trim().to_string();
                log(&format!("[WARN] Failed to install {}: {}", plugin, error));
                failed_plugins.push(format!("{plugin} ({error})"));
            }
        }
    }

    if failed_plugins.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Some mise tools failed to install: {}",
            failed_plugins.join(", ")
        ))
    }
}
