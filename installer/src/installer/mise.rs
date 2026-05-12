use crate::registry::{Category, Component};
use crate::sys::run_cmd_streaming;
use anyhow::{Result, anyhow};
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
        if let Some(tool_spec) = mise_tool_spec(c) {
            log(&format!("Installing: mise use -g {}", tool_spec));

            let result = run_cmd_streaming(&mise, &["use", "-g", &tool_spec], log.clone())?;

            if !result.success {
                let error = result.stderr.trim().to_string();
                log(&format!(
                    "[WARN] Failed to install {}: {}",
                    tool_spec, error
                ));
                failed_plugins.push(format!("{tool_spec} ({error})"));
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

fn mise_tool_spec(component: &Component) -> Option<String> {
    let Category::Mise(plugin) = &component.category else {
        return None;
    };

    let version = component.mise_version.as_deref().unwrap_or("latest");
    Some(format!("{plugin}@{version}"))
}

pub fn deactivate_mise_tools<F>(components: &[&Component], mut log: F) -> Result<()>
where
    F: FnMut(&str) + Send + 'static + Clone,
{
    if components.is_empty() {
        return Ok(());
    }

    let mise = mise_bin();
    let mut failed_plugins = Vec::new();

    for c in components {
        if let Category::Mise(ref plugin) = c.category {
            log(&format!("Uninstalling: mise unuse -g {}", plugin));

            let result = run_cmd_streaming(&mise, &["unuse", "-g", plugin], log.clone())?;

            if !result.success {
                let error = result.stderr.trim().to_string();
                log(&format!("[WARN] Failed to uninstall {}: {}", plugin, error));
                failed_plugins.push(format!("{plugin} ({error})"));
            }
        }
    }

    if failed_plugins.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Some mise tools failed to uninstall: {}",
            failed_plugins.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::mise_tool_spec;
    use crate::registry::{Category, Component, Group};

    fn mise_component(version: Option<&str>) -> Component {
        let mut component = Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Group::Languages,
            Some("rustc"),
            &["--version"],
        );
        component.mise_version = version.map(str::to_string);
        component
    }

    #[test]
    fn mise_tool_spec_should_default_to_latest() {
        assert_eq!(
            mise_tool_spec(&mise_component(None)),
            Some("rust@latest".to_string())
        );
    }

    #[test]
    fn mise_tool_spec_should_use_configured_version() {
        assert_eq!(
            mise_tool_spec(&mise_component(Some("1.85.0"))),
            Some("rust@1.85.0".to_string())
        );
    }
}
