#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    System,
    Shells,
    Editor,
    Languages,
    CliTools,
    Configurations,
    ExtraTools,
}

impl Group {
    pub fn label(&self) -> &'static str {
        match self {
            Group::System => "🖥️  System",
            Group::Shells => "🐚  Shells",
            Group::Editor => "✏️  Editor",
            Group::Languages => "🛠️  Languages",
            Group::CliTools => "🔧  CLI Tools",
            Group::Configurations => "⚙️  Configurations",
            Group::ExtraTools => "📦  Extra Tools",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Category {
    // A tool managed by mise (the string is the plugin/tool name, e.g. "rust", "cargo:ripgrep")
    Mise(String),
    // A tool/package installed via the system package manager (apt/pacman/dnf)
    SystemPackage,
    // A configuration file/folder setup that runs bash commands
    Config,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedState {
    Unknown,
    Missing,
    PathDetected(String),
    ExistingConfig,
    MiseGlobal { versions: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentAction {
    Keep,
    Install,
    Deactivate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentOutcome {
    Pending,
    Succeeded,
    Failed(String),
    AlreadyConfigured,
    Deactivated,
    Kept,
}

#[derive(Debug, Clone)]
pub struct Component {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: Category,
    pub group: Group,
    pub check_command: Option<String>,
    pub check_args: Vec<String>,
    pub mise_version: Option<String>,
    pub action: ComponentAction,
    pub observed: ObservedState,
}

impl Component {
    pub fn new(
        id: &str,
        name: &str,
        description: &str,
        category: Category,
        group: Group,
        check_command: Option<&str>,
        check_args: &[&str],
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            category,
            group,
            check_command: check_command.map(|s| s.to_string()),
            check_args: check_args.iter().map(|&s| s.to_string()).collect(),
            mise_version: None,
            action: ComponentAction::Keep,
            observed: ObservedState::Unknown,
        }
    }

    pub fn can_deactivate(&self) -> bool {
        matches!(
            (&self.category, &self.observed),
            (Category::Mise(_), ObservedState::MiseGlobal { versions }) if !versions.is_empty()
        )
    }

    pub fn set_action(&mut self, action: ComponentAction) -> bool {
        if action == ComponentAction::Deactivate && !self.can_deactivate() {
            return false;
        }
        self.action = action;
        true
    }

    pub fn toggle_install(&mut self) {
        self.action = match self.action {
            ComponentAction::Keep => ComponentAction::Install,
            ComponentAction::Install | ComponentAction::Deactivate => ComponentAction::Keep,
        };
    }

    pub fn action_label(&self) -> &'static str {
        match self.action {
            ComponentAction::Keep => "Keep",
            ComponentAction::Deactivate => "Deactivate",
            ComponentAction::Install => match (&self.category, &self.observed) {
                (Category::Mise(_), ObservedState::MiseGlobal { .. }) => "Update to latest",
                (Category::Config, ObservedState::ExistingConfig) => "Reinstall",
                _ => "Install",
            },
        }
    }

    pub fn observed_label(&self) -> String {
        match &self.observed {
            ObservedState::Unknown => "Unknown".to_string(),
            ObservedState::Missing => "Not detected".to_string(),
            ObservedState::PathDetected(version) => format!("Detected on PATH ({version})"),
            ObservedState::ExistingConfig => "Existing configuration".to_string(),
            ObservedState::MiseGlobal { versions } => {
                format!("Globally managed by mise ({})", versions.join(", "))
            }
        }
    }
}

#[derive(Debug)]
pub struct InstallPlan {
    pub system: Vec<Component>,
    pub mise: Vec<Component>,
    pub deactivate_mise: Vec<Component>,
    pub configs: Vec<Component>,
    pub kept_count: usize,
}

impl InstallPlan {
    pub fn from_components(components: &[Component]) -> Self {
        Self {
            system: collect_components(components, ComponentAction::Install, |category| {
                matches!(category, Category::SystemPackage)
            }),
            mise: collect_components(components, ComponentAction::Install, |category| {
                matches!(category, Category::Mise(_))
            }),
            deactivate_mise: collect_components(
                components,
                ComponentAction::Deactivate,
                |category| matches!(category, Category::Mise(_)),
            ),
            configs: collect_components(components, ComponentAction::Install, |category| {
                matches!(category, Category::Config)
            }),
            kept_count: components
                .iter()
                .filter(|component| component.action == ComponentAction::Keep)
                .count(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.system.is_empty()
            && self.mise.is_empty()
            && self.deactivate_mise.is_empty()
            && self.configs.is_empty()
    }

    pub fn mutation_count(&self) -> usize {
        self.system.len() + self.mise.len() + self.deactivate_mise.len() + self.configs.len()
    }

    pub fn needs_sudo(&self) -> bool {
        !self.system.is_empty()
    }

    pub fn needs_mise_install(&self) -> bool {
        !self.mise.is_empty()
            || self
                .configs
                .iter()
                .any(|component| matches!(component.id.as_str(), "config-bash" | "config-fish"))
    }

    pub fn replaces_existing_nvim_config(&self) -> bool {
        self.configs.iter().any(|component| {
            component.id == "config-nvim"
                && matches!(component.observed, ObservedState::ExistingConfig)
        })
    }
}

fn collect_components(
    components: &[Component],
    action: ComponentAction,
    predicate: impl Fn(&Category) -> bool,
) -> Vec<Component> {
    components
        .iter()
        .filter(|component| component.action == action && predicate(&component.category))
        .cloned()
        .collect()
}

pub fn get_all_components() -> Vec<Component> {
    vec![
        // ── System ───────────────────────────────────────────────────────────
        Component::new(
            "base-deps",
            "Base Dependencies",
            "Compilers, curl, git, tar, unzip",
            Category::SystemPackage,
            Group::System,
            None,
            &[],
        ),
        // ── Shells ───────────────────────────────────────────────────────────
        Component::new(
            "fish",
            "Fish Shell",
            "The user-friendly command line shell",
            Category::Mise("aqua:fish-shell/fish-shell".to_string()),
            Group::Shells,
            Some("fish"),
            &["--version"],
        ),
        // ── Editor ───────────────────────────────────────────────────────────
        Component::new(
            "neovim",
            "Neovim",
            "Vim-fork focused on extensibility",
            Category::Mise("neovim".to_string()),
            Group::Editor,
            Some("nvim"),
            &["--version"],
        ),
        // ── Languages ────────────────────────────────────────────────────────
        Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Group::Languages,
            Some("rustc"),
            &["--version"],
        ),
        Component::new(
            "node",
            "Node.js",
            "JavaScript runtime",
            Category::Mise("node".to_string()),
            Group::Languages,
            Some("node"),
            &["--version"],
        ),
        Component::new(
            "go",
            "Go",
            "Go programming language",
            Category::Mise("go".to_string()),
            Group::Languages,
            Some("go"),
            &["version"],
        ),
        Component::new(
            "python",
            "Python (uv)",
            "Python toolchain via uv",
            Category::Mise("uv".to_string()),
            Group::Languages,
            Some("uv"),
            &["--version"],
        ),
        // ── CLI Tools ────────────────────────────────────────────────────────
        Component::new(
            "fzf",
            "fzf",
            "Command-line fuzzy finder",
            Category::Mise("fzf".to_string()),
            Group::CliTools,
            Some("fzf"),
            &["--version"],
        ),
        Component::new(
            "ripgrep",
            "ripgrep (rg)",
            "Line-oriented search tool",
            Category::Mise("rg".to_string()),
            Group::CliTools,
            Some("rg"),
            &["--version"],
        ),
        Component::new(
            "fd",
            "fd",
            "Simple, fast alternative to find",
            Category::Mise("fd".to_string()),
            Group::CliTools,
            Some("fd"),
            &["--version"],
        ),
        Component::new(
            "bat",
            "bat",
            "A cat(1) clone with wings",
            Category::Mise("bat".to_string()),
            Group::CliTools,
            Some("bat"),
            &["--version"],
        ),
        Component::new(
            "eza",
            "eza",
            "Modern, maintained replacement for ls",
            Category::Mise("eza".to_string()),
            Group::CliTools,
            Some("eza"),
            &["--version"],
        ),
        Component::new(
            "glow",
            "glow",
            "Terminal based markdown reader",
            Category::Mise("glow".to_string()),
            Group::CliTools,
            Some("glow"),
            &["--version"],
        ),
        Component::new(
            "jaq",
            "jaq",
            "A jq clone focused on correctness and speed",
            Category::Mise("jaq".to_string()),
            Group::CliTools,
            Some("jaq"),
            &["--version"],
        ),
        Component::new(
            "zellij",
            "Zellij",
            "A terminal workspace with batteries included",
            Category::Mise("zellij".to_string()),
            Group::CliTools,
            Some("zellij"),
            &["--version"],
        ),
        // ── Configurations ───────────────────────────────────────────────────
        Component::new(
            "config-bash",
            "Bash Configuration",
            "Adds mise activation to ~/.bashrc",
            Category::Config,
            Group::Configurations,
            None,
            &[],
        ),
        Component::new(
            "config-fish",
            "Fish Configuration",
            "Aliases, colors, mise paths",
            Category::Config,
            Group::Configurations,
            None,
            &[],
        ),
        Component::new(
            "config-nvim",
            "LazyVim Setup",
            "LazyVim starter + OSC52 clipboard",
            Category::Config,
            Group::Configurations,
            None,
            &[],
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn config_component(id: &str) -> Component {
        Component::new(
            id,
            id,
            "configuration",
            Category::Config,
            Group::Configurations,
            None,
            &[],
        )
    }

    #[test]
    fn component_should_only_allow_deactivate_for_global_mise_tools() {
        let mut component = mise_component();
        component.observed = ObservedState::PathDetected("1.85.0".to_string());

        assert!(!component.set_action(ComponentAction::Deactivate));
    }

    #[test]
    fn component_should_allow_deactivate_for_global_mise_tools() {
        let mut component = mise_component();
        component.observed = ObservedState::MiseGlobal {
            versions: vec!["stable".to_string()],
        };

        assert!(component.set_action(ComponentAction::Deactivate));
    }

    #[test]
    fn empty_plan_should_keep_every_component() {
        let components = get_all_components();

        let plan = InstallPlan::from_components(&components);

        assert!(plan.is_empty() && plan.kept_count == components.len());
    }

    #[test]
    fn plan_should_request_sudo_only_for_system_install_actions() {
        let mut component = Component::new(
            "base-deps",
            "Base Dependencies",
            "system dependencies",
            Category::SystemPackage,
            Group::System,
            None,
            &[],
        );
        component.action = ComponentAction::Install;

        assert!(InstallPlan::from_components(&[component]).needs_sudo());
    }

    #[test]
    fn shell_config_should_add_implicit_mise_prerequisite() {
        let mut component = config_component("config-bash");
        component.action = ComponentAction::Install;

        assert!(InstallPlan::from_components(&[component]).needs_mise_install());
    }

    #[test]
    fn nvim_config_should_not_add_mise_prerequisite() {
        let mut component = config_component("config-nvim");
        component.action = ComponentAction::Install;

        assert!(!InstallPlan::from_components(&[component]).needs_mise_install());
    }

    #[test]
    fn review_should_warn_when_existing_nvim_config_will_be_replaced() {
        let mut component = config_component("config-nvim");
        component.action = ComponentAction::Install;
        component.observed = ObservedState::ExistingConfig;

        assert!(InstallPlan::from_components(&[component]).replaces_existing_nvim_config());
    }

    #[test]
    fn install_action_should_describe_global_mise_tool_as_update() {
        let mut component = mise_component();
        component.action = ComponentAction::Install;
        component.observed = ObservedState::MiseGlobal {
            versions: vec!["1.85".to_string()],
        };

        assert_eq!(component.action_label(), "Update to latest");
    }

    #[test]
    fn plan_should_partition_deactivation_separately_from_install() {
        let mut component = mise_component();
        component.observed = ObservedState::MiseGlobal {
            versions: vec!["stable".to_string()],
        };
        component.action = ComponentAction::Deactivate;

        let plan = InstallPlan::from_components(&[component]);

        assert!(plan.mise.is_empty() && plan.deactivate_mise.len() == 1);
    }
}
