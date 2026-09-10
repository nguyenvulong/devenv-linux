use crate::registry::{Category, Component, ObservedState};
use crate::sys::run_cmd_streaming;
use anyhow::{Context, Result, anyhow};
use serde_json::Value;
use std::{fs, process::Command};

const MISE_BOOTSTRAP: &str = r#"set -eu
devenv_script=$(mktemp)
trap 'rm -f "$devenv_script"' EXIT
curl -fsSL https://mise.run -o "$devenv_script"
test -s "$devenv_script"
sh "$devenv_script"
"#;

pub fn install_mise<F>(mut log: F) -> Result<()>
where
    F: FnMut(&str) + Send + 'static,
{
    log("Checking for mise...");
    let existing_mise = mise_bin();
    if existing_mise != "mise" || crate::sys::check_command_exists("mise") {
        log("mise is already installed.");
        return Ok(());
    }

    log("Installing mise...");
    let result = run_cmd_streaming("sh", &["-c", MISE_BOOTSTRAP], log)?;
    if result.success {
        let output = Command::new(mise_bin())
            .arg("--version")
            .output()
            .context("mise bootstrap completed but mise could not be executed")?;
        if !output.status.success() {
            return Err(anyhow!(
                "mise bootstrap completed but mise verification failed"
            ));
        }
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

pub fn global_mise_versions(tool: &str) -> Result<Vec<String>> {
    let mise = mise_bin();
    if mise == "mise" && !crate::sys::check_command_exists("mise") {
        return Ok(Vec::new());
    }

    let output = Command::new(&mise)
        .args(["ls", "--global", "--json", tool])
        .output()
        .with_context(|| format!("failed to inspect global mise tool {tool}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("mise ls failed for {tool}: {}", stderr.trim()));
    }

    parse_global_mise_versions(tool, &String::from_utf8_lossy(&output.stdout))
}

fn parse_global_mise_versions(tool: &str, json: &str) -> Result<Vec<String>> {
    let value: Value = serde_json::from_str(json)
        .with_context(|| format!("invalid JSON from mise ls for {tool}"))?;
    let mut versions = Vec::new();
    match value {
        Value::Array(entries) => append_global_versions(tool, &entries, &mut versions)?,
        Value::Object(object) => {
            for entries in object.values() {
                let entries = entries
                    .as_array()
                    .ok_or_else(|| anyhow!("mise ls JSON entries for {tool} were not an array"))?;
                append_global_versions(tool, entries, &mut versions)?;
            }
        }
        _ => {
            return Err(anyhow!(
                "mise ls JSON for {tool} was not an array or object"
            ));
        }
    }

    Ok(versions)
}

fn append_global_versions(tool: &str, entries: &[Value], versions: &mut Vec<String>) -> Result<()> {
    for entry in entries {
        let version = entry
            .get("requested_version")
            .or_else(|| entry.get("version"))
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("mise ls JSON entry for {tool} had no version"))?;
        if !versions.iter().any(|existing| existing == version) {
            versions.push(version.to_string());
        }
    }
    Ok(())
}

fn deactivation_specs(component: &Component) -> Vec<String> {
    let (Category::Mise(tool), ObservedState::MiseGlobal { versions }) =
        (&component.category, &component.observed)
    else {
        return Vec::new();
    };

    versions
        .iter()
        .map(|version| format!("{tool}@{version}"))
        .collect()
}

fn deactivation_args(spec: &str) -> Vec<String> {
    vec![
        "unuse".to_string(),
        "--global".to_string(),
        "--no-prune".to_string(),
        spec.to_string(),
    ]
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
    let mut failed_versions = Vec::new();

    for component in components {
        let specs = deactivation_specs(component);
        if specs.is_empty() {
            failed_versions.push(format!(
                "{} (no globally configured versions were observed)",
                component.id
            ));
            continue;
        }

        for spec in specs {
            let args = deactivation_args(&spec);
            let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
            log(&format!(
                "Deactivating: mise unuse --global --no-prune {spec}"
            ));

            match run_cmd_streaming(&mise, &arg_refs, log.clone()) {
                Ok(result) if result.success => {}
                Ok(result) => {
                    let error = result.stderr.trim().to_string();
                    log(&format!("[WARN] Failed to deactivate {spec}: {error}"));
                    failed_versions.push(format!("{spec} ({error})"));
                }
                Err(error) => {
                    log(&format!("[WARN] Failed to deactivate {spec}: {error}"));
                    failed_versions.push(format!("{spec} ({error})"));
                }
            }
        }
    }

    if failed_versions.is_empty() {
        Ok(())
    } else {
        Err(anyhow!(
            "Some mise versions failed to deactivate: {}",
            failed_versions.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        deactivate_mise_tools, deactivation_args, deactivation_specs, mise_tool_spec,
        parse_global_mise_versions,
    };
    use crate::registry::{Category, Component, Group, ObservedState};

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

    #[test]
    fn global_mise_json_should_prefer_requested_versions() {
        let json = r#"[
                {"version":"1.85.1","requested_version":"1.85"},
                {"version":"stable","requested_version":"stable"}
            ]"#;

        assert_eq!(
            parse_global_mise_versions("rust", json).expect("JSON should parse"),
            vec!["1.85".to_string(), "stable".to_string()]
        );
    }

    #[test]
    fn global_mise_json_should_fall_back_to_resolved_versions() {
        let json = r#"{"rust":[{"version":"1.85.1"}]}"#;

        assert_eq!(
            parse_global_mise_versions("rust", json).expect("JSON should parse"),
            vec!["1.85.1".to_string()]
        );
    }

    #[test]
    fn global_mise_json_should_reject_missing_versions() {
        let error = parse_global_mise_versions("rust", r#"{"rust":[{}]}"#)
            .expect_err("missing version should fail");

        assert!(error.to_string().contains("had no version"));
    }

    #[test]
    fn deactivation_should_target_every_global_requested_version() {
        let mut component = mise_component(None);
        component.observed = ObservedState::MiseGlobal {
            versions: vec!["1.84".to_string(), "stable".to_string()],
        };

        assert_eq!(
            deactivation_specs(&component),
            vec!["rust@1.84".to_string(), "rust@stable".to_string()]
        );
    }

    #[test]
    fn deactivation_args_should_keep_cache_and_target_global_config() {
        assert_eq!(
            deactivation_args("rust@stable"),
            vec![
                "unuse".to_string(),
                "--global".to_string(),
                "--no-prune".to_string(),
                "rust@stable".to_string(),
            ]
        );
    }

    #[test]
    fn deactivation_should_reject_unproven_global_state() {
        let component = mise_component(None);

        let error = deactivate_mise_tools(&[&component], |_| {})
            .expect_err("unproven global ownership should fail");

        assert!(
            error
                .to_string()
                .contains("no globally configured versions")
        );
    }
}
