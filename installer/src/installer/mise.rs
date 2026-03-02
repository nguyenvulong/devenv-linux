use crate::registry::{Component, Category};
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

pub fn activate_mise_tools<F>(components: &[&Component], mut log: F) -> Result<(), String>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    let mut args = vec!["use".to_string(), "-g".to_string()];
    
    for c in components {
        if let Category::Mise(ref plugin) = c.category {
            args.push(format!("{}@latest", plugin));
        }
    }

    if args.len() <= 2 {
        log("No mise components to install.");
        return Ok(());
    }

    let joined_args = args.join(" ");
    log(&format!("Running: mise {}", joined_args));

    // We must ensure ~/.local/bin/mise exists or we just use `mise` if it's on PATH
    let mut mise_path = "mise".to_string();
    if !crate::sys::check_command_exists("mise") {
        let home_mise = format!("{}/.local/bin/mise", std::env::var("HOME").unwrap_or_default());
        if fs::metadata(&home_mise).is_ok() {
            mise_path = home_mise;
        }
    }

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    let result = run_cmd_streaming(&mise_path, &args_ref, log);
    
    if result.success {
        Ok(())
    } else {
        Err(format!("Failed to install mise tools: {}", result.stderr))
    }
}
