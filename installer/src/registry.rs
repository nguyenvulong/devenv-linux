// std imports not needed here

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
    UpdateAvailable { current: String, latest: String },
    Failed(String),
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
        check_command: Option<&str>,
        check_args: &[&str],
    ) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: description.to_string(),
            category,
            check_command: check_command.map(|s| s.to_string()),
            check_args: check_args.iter().map(|&s| s.to_string()).collect(),
            state: SelectionState::Selected,
            status: InstallStatus::Checking,
        }
    }
}

pub fn get_all_components() -> Vec<Component> {
    vec![
        // --- System Packages ---
        Component::new(
            "base-deps",
            "Base Dependencies",
            "Compilers, curl, git, tar, unzip",
            Category::SystemPackage,
            None, // Complex check, we'll assume NotInstalled/Checking handled differently or just always run
            &[],
        ),
        Component::new(
            "tmux",
            "Tmux",
            "Terminal multiplexer",
            Category::SystemPackage,
            Some("tmux"),
            &["-V"],
        ),
        // --- Mise Tools ---
        Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Some("rustc"),
            &["--version"],
        ),
        Component::new(
            "node",
            "Node.js",
            "JavaScript runtime",
            Category::Mise("node".to_string()),
            Some("node"),
            &["--version"],
        ),
        Component::new(
            "go",
            "Go",
            "Go programming language",
            Category::Mise("go".to_string()),
            Some("go"),
            &["version"],
        ),
        Component::new(
            "python",
            "Python (uv)",
            "Python toolchain via uv",
            Category::Mise("uv".to_string()),
            Some("uv"),
            &["--version"],
        ),
        Component::new(
            "neovim",
            "Neovim",
            "Vim-fork focused on extensibility",
            Category::Mise("neovim".to_string()),
            Some("nvim"),
            &["--version"],
        ),
        Component::new(
            "fzf",
            "fzf",
            "Command-line fuzzy finder",
            Category::Mise("fzf".to_string()),
            Some("fzf"),
            &["--version"],
        ),
        Component::new(
            "ripgrep",
            "ripgrep (rg)",
            "Line-oriented search tool",
            Category::Mise("rg".to_string()),
            Some("rg"),
            &["--version"],
        ),
        Component::new(
            "fd",
            "fd",
            "Simple, fast and user-friendly alternative to find",
            Category::Mise("fd".to_string()),
            Some("fd"),
            &["--version"],
        ),
        Component::new(
            "bat",
            "bat",
            "A cat(1) clone with wings",
            Category::Mise("bat".to_string()),
            Some("bat"),
            &["--version"],
        ),
        Component::new(
            "eza",
            "eza",
            "Modern, maintained replacement for ls",
            Category::Mise("eza".to_string()),
            Some("eza"),
            &["--version"],
        ),
        Component::new(
            "glow",
            "glow",
            "Terminal based markdown reader",
            Category::Mise("glow".to_string()),
            Some("glow"),
            &["--version"],
        ),
        Component::new(
            "jaq",
            "jaq",
            "A jq clone focused on correctness, speed and simplicity",
            Category::Mise("jaq".to_string()),
            Some("jaq"),
            &["--version"],
        ),
        Component::new(
            "nushell",
            "Nushell",
            "A new type of shell",
            Category::Mise("aqua:nushell/nushell".to_string()),
            Some("nu"),
            &["--version"],
        ),
        Component::new(
            "fish",
            "Fish Shell",
            "The user-friendly command line shell",
            Category::Mise("aqua:fish-shell/fish-shell".to_string()),
            Some("fish"),
            &["--version"],
        ),
        // --- Configurations ---
        Component::new(
            "config-fish",
            "Fish Configuration",
            "Aliases, colors, paths",
            Category::Config,
            None,
            &[],
        ),
        Component::new(
            "config-tmux",
            "Tmux Configuration",
            "oh-my-tmux setup",
            Category::Config,
            None,
            &[],
        ),
        Component::new(
            "config-nushell",
            "Nushell Configuration",
            "Adds mise shims to PATH in env.nu",
            Category::Config,
            None,
            &[],
        ),
        Component::new(
            "config-nvim",
            "LazyVim Setup",
            "LazyVim starter + OSC52 clipboard",
            Category::Config,
            None,
            &[],
        ),
    ]
}
