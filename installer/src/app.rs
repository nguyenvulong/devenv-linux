use crate::manifest::{self, ManifestTool};
use crate::registry::{
    get_all_components, Category, Component, Group, InstallStatus, SelectionState,
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize},
    Arc, Mutex,
};

#[derive(PartialEq, Eq)]
pub enum Screen {
    Selection,
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
    pub should_quit: bool,
    pub manifest_tools: Vec<ManifestTool>,
    pub search_query: String,
    pub search_results: Vec<ManifestTool>,
    pub search_cursor: usize,
}

impl App {
    pub fn new() -> App {
        let home = std::env::var("HOME").unwrap_or_default();
        let mut components = get_all_components();

        for c in &mut components {
            match &c.category {
                Category::Config => {
                    let config_exists = match c.id.as_str() {
                        "config-nvim" => {
                            std::path::Path::new(&format!("{}/.config/nvim", home)).exists()
                        }
                        "config-bash" => std::fs::read_to_string(format!("{}/.bashrc", home))
                            .map(|s| s.contains("mise activate bash"))
                            .unwrap_or(false),
                        "config-fish" => {
                            std::fs::read_to_string(format!("{}/.config/fish/config.fish", home))
                                .map(|s| s.contains("mise activate fish"))
                                .unwrap_or(false)
                        }
                        _ => false,
                    };
                    if config_exists {
                        c.state = SelectionState::KeepAsIs;
                        c.status = InstallStatus::Installed("Exists".to_string());
                    } else {
                        c.state = SelectionState::Selected;
                        c.status = InstallStatus::NotInstalled;
                    }
                }
                Category::Mise(tool) => {
                    if let Some(v) = crate::sys::get_mise_tool_version(tool) {
                        c.state = SelectionState::Unselected;
                        c.status = InstallStatus::Installed(v);
                    } else if let Some(cmd) = &c.check_command {
                        if crate::sys::check_command_exists(cmd) {
                            c.state = SelectionState::Unselected;
                            c.status = InstallStatus::Installed("Detected".to_string());
                        } else {
                            c.state = SelectionState::Unselected;
                            c.status = InstallStatus::NotInstalled;
                        }
                    } else {
                        c.state = SelectionState::Unselected;
                        c.status = InstallStatus::NotInstalled;
                    }
                }
                Category::SystemPackage => {
                    if let Some(cmd) = &c.check_command {
                        if crate::sys::check_command_exists(cmd) {
                            c.state = SelectionState::Unselected;
                            let args: Vec<&str> = c.check_args.iter().map(|s| s.as_str()).collect();
                            if !args.is_empty() {
                                if let Some(version) = crate::sys::get_command_version(cmd, &args) {
                                    c.status = InstallStatus::Installed(version);
                                } else {
                                    c.status = InstallStatus::Installed("Detected".to_string());
                                }
                            } else {
                                c.status = InstallStatus::Installed("Detected".to_string());
                            }
                        } else {
                            c.state = SelectionState::Unselected;
                            c.status = InstallStatus::NotInstalled;
                        }
                    } else {
                        c.state = SelectionState::Unselected;
                        c.status = InstallStatus::NotInstalled;
                    }
                }
            }
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

        App {
            components,
            cursor: 0,
            screen: Screen::Selection,
            logs: Arc::new(Mutex::new(Vec::new())),
            install_done: Arc::new(AtomicBool::new(false)),
            install_index: Arc::new(AtomicUsize::new(0)),
            should_quit: false,
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

    pub fn toggle_selection(&mut self) {
        let c = &mut self.components[self.cursor];
        if matches!(c.category, Category::Config) && c.status != InstallStatus::NotInstalled {
            c.state = match c.state {
                SelectionState::Selected => SelectionState::KeepAsIs,
                SelectionState::KeepAsIs => SelectionState::Unselected,
                SelectionState::Unselected => SelectionState::Selected,
            };
        } else {
            c.state = match c.state {
                SelectionState::Selected => SelectionState::Unselected,
                _ => SelectionState::Selected,
            };
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

        let already_exists = self
            .components
            .iter()
            .any(|c| matches!(&c.category, Category::Mise(id) if id == &tool.mise_id));
        if already_exists {
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

        if crate::sys::check_command_exists(&check_cmd) {
            new_comp.state = SelectionState::Unselected;
            new_comp.status = InstallStatus::Installed("Detected".to_string());
        } else {
            new_comp.state = SelectionState::Unselected;
            new_comp.status = InstallStatus::NotInstalled;
        }

        self.components.push(new_comp);
        self.cursor = self.components.len() - 1;
    }

    pub fn has_selected_system_packages(&self) -> bool {
        self.components.iter().any(|component| {
            component.state == SelectionState::Selected
                && matches!(component.category, Category::SystemPackage)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use crate::registry::{Category, Component, Group, SelectionState};

    #[test]
    fn has_selected_system_packages_should_only_return_true_for_selected_system_items() {
        let mut system = Component::new(
            "base-deps",
            "Base Dependencies",
            "Compilers, curl, git, tar, unzip",
            Category::SystemPackage,
            Group::System,
            None,
            &[],
        );
        system.state = SelectionState::Selected;

        let mut app = App::new();
        app.components = vec![system];

        assert!(app.has_selected_system_packages());
    }

    #[test]
    fn has_selected_system_packages_should_ignore_unselected_items() {
        let mut system = Component::new(
            "base-deps",
            "Base Dependencies",
            "Compilers, curl, git, tar, unzip",
            Category::SystemPackage,
            Group::System,
            None,
            &[],
        );
        system.state = SelectionState::Unselected;

        let mut app = App::new();
        app.components = vec![system];

        assert!(!app.has_selected_system_packages());
    }
}
