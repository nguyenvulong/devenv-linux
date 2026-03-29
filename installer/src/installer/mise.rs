use crate::registry::{Category, Component};
use crate::sys::run_cmd_streaming;
use std::fs;

pub fn install_mise<F>(mut log: F) -> Result<(), String>
where
    F: FnMut(&str) + Send + 'static,
{
    log("Checking for mise...");
    if crate::sys::check_command_exists("mise") {
        log("mise is already installed.");
        return Ok(());
    }

    log("Installing mise...");
    let result = run_cmd_streaming("sh", &["-c", "curl https://mise.run | sh"], log);
    if result.success {
        Ok(())
    } else {
        Err(format!("Failed to install mise: {}", result.stderr))
    }
}

/// Resolve the path to the mise binary.
/// mise installs itself to ~/.local/bin/mise, which may not be on PATH yet.
pub fn mise_bin() -> String {
    if crate::sys::check_command_exists("mise") {
        return "mise".to_string();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let home_mise = format!("{}/.local/bin/mise", home);
    if fs::metadata(&home_mise).is_ok() {
        return home_mise;
    }
    "mise".to_string() // last resort — will fail with a clear error
}

pub fn activate_mise_tools<F>(components: &[&Component], mut log: F) -> Result<(), String>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    if components.is_empty() {
        log("No mise components to install.");
        return Ok(());
    }

    let mise = mise_bin();

    // Install each tool individually so a single unknown-registry name
    // does not abort the entire installation batch.
    let mut any_error = false;
    for c in components {
        if let Category::Mise(ref plugin) = c.category {
            let tool_spec = format!("{}@latest", plugin);
            log(&format!("Installing: mise use -g {}", tool_spec));

            let result = run_cmd_streaming(&mise, &["use", "-g", &tool_spec], log.clone());

            if !result.success {
                log(&format!(
                    "[WARN] Failed to install {}: {}",
                    plugin,
                    result.stderr.trim()
                ));
                any_error = true;
                // Continue — don't abort the loop for one bad tool.
            }
        }
    }

    if any_error {
        // Return Ok so the caller doesn't treat this as a fatal error;
        // individual [WARN] lines already surfaced the details.
        log("[WARN] Some mise tools failed to install (see above).");
    }

    Ok(())
}
