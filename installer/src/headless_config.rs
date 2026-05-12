use crate::registry::{Category, Component, SelectionState};
use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct HeadlessConfig {
    #[serde(default)]
    components: Vec<ConfiguredComponent>,
}

#[derive(Debug, Deserialize)]
struct ConfiguredComponent {
    id: String,
    enabled: bool,
    version: Option<String>,
}

pub fn components_from_file(path: &Path) -> Result<Vec<Component>> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;
    components_from_str(&raw)
}

pub(crate) fn components_from_str(raw: &str) -> Result<Vec<Component>> {
    let config: HeadlessConfig = toml::from_str(raw).context("Failed to parse headless config")?;
    let mut components = crate::registry::get_all_components();
    apply_config(config, &mut components)?;
    Ok(components)
}

fn apply_config(config: HeadlessConfig, components: &mut [Component]) -> Result<()> {
    for component in components.iter_mut() {
        component.state = SelectionState::Unselected;
        component.mise_version = None;
    }

    let component_indexes: HashMap<String, usize> = components
        .iter()
        .enumerate()
        .map(|(index, component)| (component.id.clone(), index))
        .collect();
    let mut seen = HashSet::new();

    for configured in config.components {
        if !seen.insert(configured.id.clone()) {
            bail!("Duplicate component id in config: {}", configured.id);
        }

        let Some(index) = component_indexes.get(&configured.id).copied() else {
            bail!("Unknown component id in config: {}", configured.id);
        };
        let component = &mut components[index];

        if configured.enabled {
            component.state = SelectionState::Selected;
        }

        let Some(version) = configured.version else {
            continue;
        };
        let version = version.trim();
        if version.is_empty() {
            bail!("Component {} has an empty version", configured.id);
        }
        if !matches!(component.category, Category::Mise(_)) {
            bail!(
                "Component {} is not a mise tool and cannot set version",
                configured.id
            );
        }
        if version != "latest" {
            component.mise_version = Some(version.to_string());
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::components_from_str;
    use crate::registry::{Category, SelectionState};

    fn selected_component<'a>(
        components: &'a [crate::registry::Component],
        id: &str,
    ) -> &'a crate::registry::Component {
        components
            .iter()
            .find(|component| component.id == id)
            .expect("component should exist")
    }

    #[test]
    fn config_should_select_only_enabled_components() {
        let components = components_from_str(
            r#"
            [[components]]
            id = "rust"
            enabled = true

            [[components]]
            id = "node"
            enabled = false
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            selected_component(&components, "rust").state,
            SelectionState::Selected
        );
        assert_eq!(
            selected_component(&components, "node").state,
            SelectionState::Unselected
        );
        assert_eq!(
            selected_component(&components, "go").state,
            SelectionState::Unselected
        );
    }

    #[test]
    fn config_should_default_mise_versions_to_latest() {
        let components = components_from_str(
            r#"
            [[components]]
            id = "rust"
            enabled = true
            "#,
        )
        .expect("config should parse");

        assert_eq!(selected_component(&components, "rust").mise_version, None);
    }

    #[test]
    fn config_should_preserve_pinned_mise_versions() {
        let components = components_from_str(
            r#"
            [[components]]
            id = "rust"
            enabled = true
            version = "1.85.0"
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            selected_component(&components, "rust")
                .mise_version
                .as_deref(),
            Some("1.85.0")
        );
    }

    #[test]
    fn config_should_reject_unknown_component_ids() {
        let error = components_from_str(
            r#"
            [[components]]
            id = "does-not-exist"
            enabled = true
            "#,
        )
        .expect_err("config should fail");

        assert!(error.to_string().contains("Unknown component id"));
    }

    #[test]
    fn config_should_reject_duplicate_component_ids() {
        let error = components_from_str(
            r#"
            [[components]]
            id = "rust"
            enabled = true

            [[components]]
            id = "rust"
            enabled = false
            "#,
        )
        .expect_err("config should fail");

        assert!(error.to_string().contains("Duplicate component id"));
    }

    #[test]
    fn config_should_reject_version_on_non_mise_components() {
        let error = components_from_str(
            r#"
            [[components]]
            id = "config-bash"
            enabled = true
            version = "1.0.0"
            "#,
        )
        .expect_err("config should fail");

        assert!(error.to_string().contains("cannot set version"));
    }

    #[test]
    fn config_should_treat_latest_as_default_version() {
        let components = components_from_str(
            r#"
            [[components]]
            id = "rust"
            enabled = true
            version = "latest"
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            selected_component(&components, "rust")
                .mise_version
                .as_deref(),
            None
        );
        assert!(matches!(
            selected_component(&components, "rust").category,
            Category::Mise(_)
        ));
    }

    #[test]
    fn sample_config_should_parse() {
        let components = components_from_str(include_str!("../../devenv.example.toml"))
            .expect("sample config should parse");

        assert!(
            components
                .iter()
                .all(|component| component.state == SelectionState::Selected)
        );
    }
}
