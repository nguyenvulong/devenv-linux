use crate::manifest::{self, ManifestTool};
use crate::registry::{
    Category, Component, ComponentAction, ComponentOutcome, Group, ObservedState,
    get_all_components,
};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize},
    },
};

#[derive(PartialEq, Eq)]
pub enum Screen {
    Selection,
    Review,
    Installing,
    Report,
    Search,
}

pub struct App {
    pub components: Vec<Component>,
    pub cursor: usize,
    pub screen: Screen,
    pub logs: Arc<Mutex<Vec<String>>>,
    pub install_done: Arc<AtomicBool>,
    pub install_index: Arc<AtomicUsize>,
    pub outcomes: Arc<Mutex<HashMap<String, ComponentOutcome>>>,
    pub should_quit: bool,
    pub notice: Option<String>,
    pub manifest_tools: Vec<ManifestTool>,
    pub search_query: String,
    pub search_results: Vec<ManifestTool>,
    pub search_cursor: usize,
}

impl App {
    pub fn new() -> App {
        let home = std::env::var("HOME").unwrap_or_default();
        let mut components = get_all_components();

        for component in &mut components {
            component.observed = observe_component(component, &home);
        }

        let curated = manifest::load_manifest();
        let manifest_tools = if crate::sys::check_command_exists("mise") {
            if let Some(runtime) = manifest::load_runtime_registry() {
                manifest::merge(curated, runtime)
            } else {
                curated
            }
        } else {
            curated
        };

        let search_results = manifest_tools.clone();
        let outcomes = components
            .iter()
            .map(|component| (component.id.clone(), ComponentOutcome::Kept))
            .collect();

        App {
            components,
            cursor: 0,
            screen: Screen::Selection,
            logs: Arc::new(Mutex::new(Vec::new())),
            install_done: Arc::new(AtomicBool::new(false)),
            install_index: Arc::new(AtomicUsize::new(0)),
            outcomes: Arc::new(Mutex::new(outcomes)),
            should_quit: false,
            notice: None,
            manifest_tools,
            search_query: String::new(),
            search_results,
            search_cursor: 0,
        }
    }

    pub fn next(&mut self) {
        if self.cursor + 1 < self.components.len() {
            self.cursor += 1;
        }
    }

    pub fn previous(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn toggle_current(&mut self) {
        self.components[self.cursor].toggle_install();
        self.notice = None;
    }

    pub fn set_current_action(&mut self, action: ComponentAction) {
        if self.components[self.cursor].set_action(action) {
            self.notice = None;
        } else {
            self.notice =
                Some("Only globally configured mise tools can be deactivated.".to_string());
        }
    }

    pub fn install_all(&mut self) {
        for component in &mut self.components {
            component.action = ComponentAction::Install;
        }
        self.notice = None;
    }

    pub fn keep_all(&mut self) {
        for component in &mut self.components {
            component.action = ComponentAction::Keep;
        }
        self.notice = None;
    }

    pub fn prepare_installation(&mut self) {
        if let Ok(mut logs) = self.logs.lock() {
            logs.clear();
        }
        self.install_done
            .store(false, std::sync::atomic::Ordering::Release);
        self.install_index
            .store(0, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut outcomes) = self.outcomes.lock() {
            for component in &self.components {
                let outcome = match component.action {
                    ComponentAction::Keep => ComponentOutcome::Kept,
                    ComponentAction::Install | ComponentAction::Deactivate => {
                        ComponentOutcome::Pending
                    }
                };
                outcomes.insert(component.id.clone(), outcome);
            }
        }
    }

    pub fn update_search(&mut self) {
        self.search_results = manifest::search(&self.manifest_tools, &self.search_query);
        self.search_cursor = 0;
    }

    pub fn search_next(&mut self) {
        if self.search_cursor + 1 < self.search_results.len() {
            self.search_cursor += 1;
        }
    }

    pub fn search_previous(&mut self) {
        if self.search_cursor > 0 {
            self.search_cursor -= 1;
        }
    }

    pub fn add_search_result(&mut self) {
        let Some(tool) = self.search_results.get(self.search_cursor).cloned() else {
            return;
        };

        if let Some(index) = self.components.iter().position(
            |component| matches!(&component.category, Category::Mise(id) if id == &tool.mise_id),
        ) {
            self.components[index].action = ComponentAction::Install;
            self.cursor = index;
            self.notice = None;
            return;
        }

        let check_cmd = tool.name.clone();

        let mut new_comp = Component::new(
            &format!("extra-{}", tool.name),
            &tool.name,
            &tool.description,
            Category::Mise(tool.mise_id),
            Group::ExtraTools,
            Some(&check_cmd),
            &["--version"],
        );
        let home = std::env::var("HOME").unwrap_or_default();
        new_comp.observed = observe_component(&new_comp, &home);
        new_comp.action = ComponentAction::Install;

        self.components.push(new_comp);
        self.cursor = self.components.len() - 1;
        if let Ok(mut outcomes) = self.outcomes.lock() {
            outcomes.insert(
                self.components[self.cursor].id.clone(),
                ComponentOutcome::Pending,
            );
        }
        self.notice = None;
    }
}

fn observe_component(component: &Component, home: &str) -> ObservedState {
    match &component.category {
        Category::Config => {
            let exists = match component.id.as_str() {
                "config-nvim" => std::path::Path::new(home).join(".config/nvim").exists(),
                "config-bash" => {
                    std::fs::read_to_string(std::path::Path::new(home).join(".bashrc"))
                        .is_ok_and(|contents| contents.contains("mise activate bash"))
                }
                "config-fish" => std::fs::read_to_string(
                    std::path::Path::new(home).join(".config/fish/config.fish"),
                )
                .is_ok_and(|contents| contents.contains("mise activate fish")),
                _ => false,
            };
            if exists {
                ObservedState::ExistingConfig
            } else {
                ObservedState::Missing
            }
        }
        Category::Mise(tool) => {
            observed_mise_state(crate::installer::mise::global_mise_versions(tool), || {
                observe_command(component)
            })
        }
        Category::SystemPackage => observe_command(component).unwrap_or(ObservedState::Unknown),
    }
}

fn observed_mise_state<E>(
    global_versions: Result<Vec<String>, E>,
    path_fallback: impl FnOnce() -> Option<ObservedState>,
) -> ObservedState {
    match global_versions {
        Ok(versions) if !versions.is_empty() => ObservedState::MiseGlobal { versions },
        Ok(_) => path_fallback().unwrap_or(ObservedState::Missing),
        Err(_) => path_fallback().unwrap_or(ObservedState::Unknown),
    }
}

fn observe_command(component: &Component) -> Option<ObservedState> {
    let command = component.check_command.as_deref()?;
    if !crate::sys::check_command_exists(command) {
        return None;
    }

    let args: Vec<&str> = component.check_args.iter().map(String::as_str).collect();
    let version = if args.is_empty() {
        "Detected".to_string()
    } else {
        crate::sys::get_command_version(command, &args).unwrap_or_else(|| "Detected".to_string())
    };
    Some(ObservedState::PathDetected(version))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(components: Vec<Component>) -> App {
        let outcomes = components
            .iter()
            .map(|component| (component.id.clone(), ComponentOutcome::Kept))
            .collect();

        App {
            components,
            cursor: 0,
            screen: Screen::Selection,
            logs: Arc::new(Mutex::new(Vec::new())),
            install_done: Arc::new(AtomicBool::new(false)),
            install_index: Arc::new(AtomicUsize::new(0)),
            outcomes: Arc::new(Mutex::new(outcomes)),
            should_quit: false,
            notice: None,
            manifest_tools: Vec::new(),
            search_query: String::new(),
            search_results: Vec::new(),
            search_cursor: 0,
        }
    }

    fn mise_component() -> Component {
        Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Group::Languages,
            Some("rustc"),
            &["--version"],
        )
    }

    #[test]
    fn built_in_components_should_default_to_keep() {
        assert!(
            get_all_components()
                .iter()
                .all(|component| component.action == ComponentAction::Keep)
        );
    }

    #[test]
    fn space_toggle_should_switch_keep_and_install() {
        let mut app = test_app(vec![mise_component()]);

        app.toggle_current();

        assert_eq!(app.components[0].action, ComponentAction::Install);
    }

    #[test]
    fn space_toggle_should_return_deactivate_to_keep() {
        let mut component = mise_component();
        component.observed = ObservedState::MiseGlobal {
            versions: vec!["stable".to_string()],
        };
        component.action = ComponentAction::Deactivate;
        let mut app = test_app(vec![component]);

        app.toggle_current();

        assert_eq!(app.components[0].action, ComponentAction::Keep);
    }

    #[test]
    fn deactivate_should_be_rejected_for_path_only_tools() {
        let mut component = mise_component();
        component.observed = ObservedState::PathDetected("1.85.0".to_string());
        let mut app = test_app(vec![component]);

        app.set_current_action(ComponentAction::Deactivate);

        assert_eq!(app.components[0].action, ComponentAction::Keep);
    }

    #[test]
    fn bulk_actions_should_never_select_deactivate() {
        let mut app = test_app(vec![mise_component()]);

        app.install_all();
        app.keep_all();

        assert_eq!(app.components[0].action, ComponentAction::Keep);
    }

    #[test]
    fn search_result_should_set_existing_tool_to_install() {
        let mut app = test_app(vec![mise_component()]);
        app.search_results = vec![ManifestTool {
            name: "rust".to_string(),
            mise_id: "rust".to_string(),
            description: "Rust programming language".to_string(),
        }];

        app.add_search_result();

        assert_eq!(app.components[0].action, ComponentAction::Install);
    }

    #[test]
    fn mise_detection_failure_should_preserve_path_detection() {
        let observed = observed_mise_state::<&str>(Err("command failed"), || {
            Some(ObservedState::PathDetected("1.85.0".to_string()))
        });

        assert_eq!(observed, ObservedState::PathDetected("1.85.0".to_string()));
    }

    #[test]
    fn mise_detection_failure_without_path_tool_should_be_unknown() {
        let observed = observed_mise_state::<&str>(Err("command failed"), || None);

        assert_eq!(observed, ObservedState::Unknown);
    }

    #[test]
    fn successful_empty_mise_detection_without_path_tool_should_be_missing() {
        let observed = observed_mise_state::<&str>(Ok(Vec::new()), || None);

        assert_eq!(observed, ObservedState::Missing);
    }

    #[test]
    fn preparing_installation_should_record_kept_and_pending_outcomes() {
        let kept = mise_component();
        let mut install = mise_component();
        install.id = "node".to_string();
        install.action = ComponentAction::Install;
        let mut app = test_app(vec![kept, install]);

        app.prepare_installation();

        let outcomes = app
            .outcomes
            .lock()
            .expect("outcomes should be available")
            .clone();
        assert_eq!(
            (outcomes.get("rust"), outcomes.get("node"),),
            (
                Some(&ComponentOutcome::Kept),
                Some(&ComponentOutcome::Pending),
            )
        );
    }
}
