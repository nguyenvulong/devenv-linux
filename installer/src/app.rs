use crate::registry::{get_all_components, Category, Component, InstallStatus, SelectionState};
use std::sync::{Arc, Mutex};

#[derive(PartialEq, Eq)]
pub enum Screen {
    Selection,
    Installing,
    Report,
}

pub struct App {
    pub components: Vec<Component>,
    pub cursor: usize,
    pub screen: Screen,
    /// Shared log buffer written by the installer thread, read by the UI.
    pub logs: Arc<Mutex<Vec<String>>>,
    /// Set to true by the installer thread when all phases finish.
    pub install_done: Arc<Mutex<bool>>,
    /// Phase counter (0=sys, 1=mise, 2=config) for the progress gauge.
    pub install_index: Arc<Mutex<usize>>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> App {
        let home = std::env::var("HOME").unwrap_or_default();
        let mut components = get_all_components();

        // Detect already-installed tools and set sensible defaults.
        for c in &mut components {
            match &c.category {
                Category::Config => {
                    // Check whether the config directory/file already exists.
                    let config_exists = match c.id.as_str() {
                        "config-nvim" => {
                            std::path::Path::new(&format!("{}/.config/nvim", home)).exists()
                        }
                        "config-tmux" => {
                            std::path::Path::new(&format!("{}/.config/tmux", home)).exists()
                        }
                        "config-fish" => {
                            std::path::Path::new(&format!("{}/.config/fish/config.fish", home))
                                .exists()
                        }
                        "config-nushell" => {
                            // Consider it installed only if the shims line is already present.
                            std::fs::read_to_string(format!("{}/.config/nushell/env.nu", home))
                                .map(|s| s.contains(".local/share/mise/shims"))
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
                    // 1) Check exact version managed by mise 
                    if let Some(v) = crate::sys::get_mise_tool_version(tool) {
                        c.state = SelectionState::Unselected;
                        c.status = InstallStatus::Installed(v);
                    } else if let Some(cmd) = &c.check_command {
                        // 2) Fallback: just check if command is on PATH
                        if crate::sys::check_command_exists(cmd) {
                            c.state = SelectionState::Unselected;
                            c.status = InstallStatus::Installed("Detected".to_string());
                        } else {
                            c.state = SelectionState::Selected;
                            c.status = InstallStatus::NotInstalled;
                        }
                    } else {
                        c.state = SelectionState::Selected;
                        c.status = InstallStatus::NotInstalled;
                    }
                }
                Category::SystemPackage => {
                    if let Some(cmd) = &c.check_command {
                        if crate::sys::check_command_exists(cmd) {
                            c.state = SelectionState::Unselected;
                            
                            // Try to get version using check_args
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
                            c.state = SelectionState::Selected;
                            c.status = InstallStatus::NotInstalled;
                        }
                    } else {
                        // SystemPackage base-deps: always offer to (re-)install.
                        c.state = SelectionState::Selected;
                        c.status = InstallStatus::NotInstalled;
                    }
                }
            }
        }

        App {
            components,
            cursor: 0,
            screen: Screen::Selection,
            logs: Arc::new(Mutex::new(Vec::new())),
            install_done: Arc::new(Mutex::new(false)),
            install_index: Arc::new(Mutex::new(0)),
            should_quit: false,
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
            // Cycle: Selected ➜ KeepAsIs ➜ Unselected ➜ Selected
            c.state = match c.state {
                SelectionState::Selected => SelectionState::KeepAsIs,
                SelectionState::KeepAsIs => SelectionState::Unselected,
                SelectionState::Unselected => SelectionState::Selected,
            };
        } else {
            // Binary toggle for everything else.
            c.state = match c.state {
                SelectionState::Selected => SelectionState::Unselected,
                _ => SelectionState::Selected,
            };
        }
    }
}
