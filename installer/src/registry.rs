// std imports not needed here

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
pub enum InstallStatus {
    Checking,
    NotInstalled,
    Installed(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionState {
    Selected,
    Unselected,
    // For configs where we might want to skip overwriting existing user data
    KeepAsIs,
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
    pub state: SelectionState,
    pub status: InstallStatus,
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
            state: SelectionState::Unselected,
            status: InstallStatus::Checking,
        }
    }
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
        Component::new(
            "tmux",
            "Tmux",
            "Terminal multiplexer",
            Category::SystemPackage,
            Group::System,
            Some("tmux"),
            &["-V"],
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
            "config-tmux",
            "Tmux Configuration",
            "oh-my-tmux setup",
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
