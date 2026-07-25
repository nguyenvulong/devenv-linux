use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, prelude::Backend};
use std::{
    error::Error,
    io,
    io::Write,
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::Duration,
};

mod app;
mod headless_config;
mod installer;
mod manifest;
mod registry;
mod sys;
mod theme;
mod ui;

use app::{App, Screen};
use registry::{Component, ComponentAction, ComponentOutcome, InstallPlan};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(helper) = cli_helper(&args) {
        match helper {
            CliHelper::Help => print_help(),
            CliHelper::Version => print_version(),
        }
        return Ok(());
    }

    if let Some(config_path) = headless_config_path(&args)? {
        return run_headless_config(config_path);
    }

    let headless = args.iter().any(|a| a == "--all")
        || std::env::var("CI").map(|v| v == "true").unwrap_or(false)
        || std::env::var("INSTALLER_ALL")
            .map(|v| v == "1")
            .unwrap_or(false);

    if headless {
        return run_headless();
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum CliHelper {
    Help,
    Version,
}

fn cli_helper(args: &[String]) -> Option<CliHelper> {
    args.iter().skip(1).find_map(|arg| match arg.as_str() {
        "--help" | "-h" => Some(CliHelper::Help),
        "--version" | "-v" => Some(CliHelper::Version),
        _ => None,
    })
}

fn print_help() {
    println!(
        "\
devenv-linux {}

Usage:
  devenv [OPTIONS]

Options:
      --all              Install every built-in component
  -c, --config <PATH>    Install enabled components from a TOML config
  -h, --help             Print help
  -v, --version          Print version
",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_version() {
    println!("devenv {}", env!("CARGO_PKG_VERSION"));
}

fn headless_config_path(args: &[String]) -> Result<Option<PathBuf>, Box<dyn Error>> {
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--config" || arg == "-c" {
            let Some(path) = iter.next() else {
                return Err(format!("{arg} requires a path").into());
            };
            return Ok(Some(PathBuf::from(path)));
        }

        if let Some(path) = arg.strip_prefix("--config=") {
            if path.is_empty() {
                return Err("--config requires a path".into());
            }
            return Ok(Some(PathBuf::from(path)));
        }
    }

    Ok(None)
}

fn run_headless() -> Result<(), Box<dyn Error>> {
    let mut components = registry::get_all_components();
    for c in &mut components {
        c.action = ComponentAction::Install;
    }

    run_headless_components(components, "--all mode")
}

fn run_headless_config(config_path: PathBuf) -> Result<(), Box<dyn Error>> {
    let components = headless_config::components_from_file(&config_path)?;
    run_headless_components(
        components,
        &format!("config mode: {}", config_path.display()),
    )
}

fn run_headless_components(components: Vec<Component>, mode: &str) -> Result<(), Box<dyn Error>> {
    println!("==> devenv-linux headless installer ({mode})");
    println!();

    let install_plan = InstallPlan::from_components(&components);
    if install_plan.needs_sudo() {
        println!("Some components require elevated privileges (sudo).");
        let status = Command::new("sudo").arg("-v").status()?;
        if !status.success() {
            eprintln!("sudo authentication failed. Aborting.");
            std::process::exit(1);
        }
        start_sudo_keepalive();
    }

    println!(">>> Phase 1: System Packages");
    let sys_comps: Vec<&registry::Component> = install_plan.system.iter().collect();
    if let Err(e) =
        installer::system::install_system_packages(&sys_comps, |msg| println!("{}", msg))
    {
        eprintln!("[ERROR] System packages: {}", e);
    }

    println!("\n>>> Phase 2: Mise Tools");
    let mise_ready = if install_plan.needs_mise_install() {
        match installer::mise::install_mise(|msg| println!("{}", msg)) {
            Ok(()) => true,
            Err(error) => {
                eprintln!("[ERROR] mise prerequisite: {error}");
                false
            }
        }
    } else {
        true
    };

    for component in &install_plan.mise {
        if !mise_ready {
            eprintln!("[ERROR] mise tool {}: mise is unavailable", component.id);
            continue;
        }
        if let Err(error) =
            installer::mise::activate_mise_tools(&[component], |message| println!("{message}"))
        {
            eprintln!("[ERROR] mise tool {}: {error}", component.id);
        }
    }

    println!("\n>>> Phase 3: Configurations");
    for component in &install_plan.configs {
        if config_needs_mise(component) && !mise_ready {
            eprintln!("[ERROR] config {}: mise is unavailable", component.id);
            continue;
        }
        if let Err(error) =
            installer::config::setup_config(component, |message| println!("{message}"))
        {
            eprintln!("[ERROR] config {}: {error}", component.id);
        }
    }

    println!("\n✅ All done!");
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, app: &mut App) -> Result<(), Box<dyn Error>>
where
    <B as Backend>::Error: 'static,
{
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            return Ok(());
        }

        if app.screen == Screen::Installing {
            let done = app.install_done.load(Ordering::Acquire);
            if done {
                app.screen = Screen::Report;
            }
            continue;
        }

        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
        {
            if app.screen == Screen::Selection && handle_selection_action_key(app, key.code) {
                continue;
            }

            match app.screen {
                Screen::Selection => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    KeyCode::Char('/') => {
                        app.search_query.clear();
                        app.update_search();
                        app.screen = Screen::Search;
                    }
                    KeyCode::Enter => {
                        let plan = InstallPlan::from_components(&app.components);
                        if plan.is_empty() {
                            app.notice = Some("No changes selected.".to_string());
                        } else {
                            app.notice = None;
                            app.screen = Screen::Review;
                        }
                    }
                    _ => {}
                },
                Screen::Review => match key.code {
                    KeyCode::Esc => app.screen = Screen::Selection,
                    KeyCode::Enter => {
                        let plan = InstallPlan::from_components(&app.components);
                        if plan.needs_sudo() && !ensure_sudo_credentials_for_install()? {
                            continue;
                        }

                        app.prepare_installation();
                        app.screen = Screen::Installing;
                        spawn_installation(app);
                    }
                    _ => {}
                },
                Screen::Search => match key.code {
                    KeyCode::Esc => app.screen = Screen::Selection,
                    KeyCode::Up => app.search_previous(),
                    KeyCode::Down => app.search_next(),
                    KeyCode::Enter => {
                        app.add_search_result();
                        app.screen = Screen::Selection;
                    }
                    KeyCode::Backspace => {
                        app.search_query.pop();
                        app.update_search();
                    }
                    KeyCode::Char(c) => {
                        app.search_query.push(c);
                        app.update_search();
                    }
                    _ => {}
                },
                Screen::Report => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => app.should_quit = true,
                    _ => {}
                },
                _ => {}
            }
        }
    }
}

fn handle_selection_action_key(app: &mut App, key: KeyCode) -> bool {
    match key {
        KeyCode::Char('i') => app.set_current_action(ComponentAction::Install),
        KeyCode::Char('u') => app.set_current_action(ComponentAction::Keep),
        KeyCode::Char('d') => app.set_current_action(ComponentAction::Deactivate),
        KeyCode::Char(' ') => app.toggle_current(),
        KeyCode::Char('a') => app.install_all(),
        KeyCode::Char('n') => app.keep_all(),
        _ => return false,
    }
    true
}

fn config_needs_mise(component: &Component) -> bool {
    matches!(component.id.as_str(), "config-bash" | "config-fish")
}

fn ensure_sudo_credentials_for_install() -> Result<bool, Box<dyn Error>> {
    if has_cached_sudo_credentials()? {
        start_sudo_keepalive();
        return Ok(true);
    }

    suspend_tui()?;

    println!("A planned system-package installation requires sudo.");
    println!("Please enter your sudo password to continue.");
    println!();

    let status = Command::new("sudo")
        .arg("-v")
        .status()
        .map_err(|e| format!("Failed to run sudo: {e}"))?;

    let authenticated = status.success();
    if authenticated {
        start_sudo_keepalive();
    } else {
        println!("sudo authentication was cancelled or failed.");
        println!("Press Enter to return to the installer.");
        io::stdout().flush()?;

        let mut input = String::new();
        let _ = io::stdin().read_line(&mut input);
    }

    resume_tui()?;
    Ok(authenticated)
}

fn has_cached_sudo_credentials() -> Result<bool, Box<dyn Error>> {
    let status = Command::new("sudo")
        .args(["-n", "true"])
        .status()
        .map_err(|e| format!("Failed to check sudo credentials: {e}"))?;

    Ok(status.success())
}

fn start_sudo_keepalive() {
    thread::spawn(|| {
        loop {
            thread::sleep(Duration::from_secs(50));
            let Ok(status) = Command::new("sudo").args(["-n", "true"]).status() else {
                break;
            };

            if !status.success() {
                break;
            }

            let _ = Command::new("sudo").arg("-v").output();
        }
    });
}

fn suspend_tui() -> Result<(), Box<dyn Error>> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
    Ok(())
}

fn resume_tui() -> Result<(), Box<dyn Error>> {
    enable_raw_mode()?;
    execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    Ok(())
}

fn spawn_installation(app: &mut App) {
    let logs = Arc::clone(&app.logs);
    let done_flag = Arc::clone(&app.install_done);
    let install_index = Arc::clone(&app.install_index);
    let outcomes = Arc::clone(&app.outcomes);
    let install_plan = InstallPlan::from_components(&app.components);

    let make_log = move |logs: Arc<Mutex<Vec<String>>>| {
        move |msg: &str| {
            if let Ok(mut guard) = logs.lock() {
                guard.push(msg.to_string());
            }
        }
    };

    thread::spawn(move || {
        install_index.store(0, Ordering::Relaxed);
        push_log(&logs, ">>> Phase 1: System Packages");

        let sys_comps: Vec<&Component> = install_plan.system.iter().collect();
        let system_result =
            installer::system::install_system_packages(&sys_comps, make_log(logs.clone()));
        match system_result {
            Ok(()) => {
                for component in &install_plan.system {
                    set_outcome(&outcomes, &component.id, ComponentOutcome::Succeeded);
                }
            }
            Err(error) => {
                push_log(&logs, format!("[ERROR] System packages: {error}"));
                for component in &install_plan.system {
                    set_outcome(
                        &outcomes,
                        &component.id,
                        ComponentOutcome::Failed(error.to_string()),
                    );
                }
            }
        }

        install_index.store(1, Ordering::Relaxed);
        push_log(&logs, "\n>>> Phase 2: Mise Tools");

        for component in &install_plan.deactivate_mise {
            match installer::mise::deactivate_mise_tools(&[component], make_log(logs.clone())) {
                Ok(()) => set_outcome(&outcomes, &component.id, ComponentOutcome::Deactivated),
                Err(error) => {
                    push_log(
                        &logs,
                        format!("[ERROR] deactivate {}: {error}", component.id),
                    );
                    set_outcome(
                        &outcomes,
                        &component.id,
                        ComponentOutcome::Failed(error.to_string()),
                    );
                }
            }
        }

        let mise_ready = if install_plan.needs_mise_install() {
            match installer::mise::install_mise(make_log(logs.clone())) {
                Ok(()) => true,
                Err(error) => {
                    push_log(&logs, format!("[ERROR] mise prerequisite: {error}"));
                    false
                }
            }
        } else {
            true
        };

        for component in &install_plan.mise {
            if !mise_ready {
                set_outcome(
                    &outcomes,
                    &component.id,
                    ComponentOutcome::Failed("mise prerequisite failed".to_string()),
                );
                continue;
            }
            match installer::mise::activate_mise_tools(&[component], make_log(logs.clone())) {
                Ok(()) => set_outcome(&outcomes, &component.id, ComponentOutcome::Succeeded),
                Err(error) => {
                    push_log(
                        &logs,
                        format!("[ERROR] mise tool {}: {error}", component.id),
                    );
                    set_outcome(
                        &outcomes,
                        &component.id,
                        ComponentOutcome::Failed(error.to_string()),
                    );
                }
            }
        }

        install_index.store(2, Ordering::Relaxed);
        push_log(&logs, "\n>>> Phase 3: Configurations");

        for component in &install_plan.configs {
            if config_needs_mise(component) && !mise_ready {
                set_outcome(
                    &outcomes,
                    &component.id,
                    ComponentOutcome::Failed("mise prerequisite failed".to_string()),
                );
                continue;
            }

            match installer::config::setup_config(component, make_log(logs.clone())) {
                Ok(installer::config::ConfigOutcome::Changed) => {
                    set_outcome(&outcomes, &component.id, ComponentOutcome::Succeeded)
                }
                Ok(installer::config::ConfigOutcome::AlreadyConfigured) => set_outcome(
                    &outcomes,
                    &component.id,
                    ComponentOutcome::AlreadyConfigured,
                ),
                Err(error) => {
                    push_log(&logs, format!("[ERROR] config {}: {error}", component.id));
                    set_outcome(
                        &outcomes,
                        &component.id,
                        ComponentOutcome::Failed(error.to_string()),
                    );
                }
            }
        }

        push_log(&logs, "\n✅ All done! Press Enter to view the summary.");
        done_flag.store(true, Ordering::Release);
    });
}

fn push_log(logs: &Arc<Mutex<Vec<String>>>, message: impl Into<String>) {
    if let Ok(mut guard) = logs.lock() {
        guard.push(message.into());
    }
}

fn set_outcome(
    outcomes: &Arc<Mutex<std::collections::HashMap<String, ComponentOutcome>>>,
    component_id: &str,
    outcome: ComponentOutcome,
) {
    if let Ok(mut outcomes) = outcomes.lock() {
        outcomes.insert(component_id.to_string(), outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CliHelper, InstallPlan, cli_helper, handle_selection_action_key, headless_config_path,
    };
    use crate::app::{App, Screen};
    use crate::registry::{
        Category, Component, ComponentAction, ComponentOutcome, Group, ObservedState,
    };
    use crossterm::event::KeyCode;
    use std::{
        collections::HashMap,
        path::PathBuf,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize},
        },
    };

    fn action_test_app(component: Component) -> App {
        let outcomes = HashMap::from([(component.id.clone(), ComponentOutcome::Kept)]);
        App {
            components: vec![component],
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

    fn action_test_component() -> Component {
        let mut component = Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Group::Languages,
            Some("rustc"),
            &["--version"],
        );
        component.observed = ObservedState::MiseGlobal {
            versions: vec!["stable".to_string()],
        };
        component
    }

    #[test]
    fn selection_action_keys_should_map_to_explicit_actions() {
        let cases = [
            (KeyCode::Char('i'), ComponentAction::Install),
            (KeyCode::Char('u'), ComponentAction::Keep),
            (KeyCode::Char('d'), ComponentAction::Deactivate),
        ];
        let mut actual = Vec::new();

        for (key, expected) in cases {
            let mut app = action_test_app(action_test_component());
            app.components[0].action = if expected == ComponentAction::Keep {
                ComponentAction::Install
            } else {
                ComponentAction::Keep
            };
            assert!(handle_selection_action_key(&mut app, key));
            actual.push(app.components[0].action);
        }

        assert_eq!(
            actual,
            vec![
                ComponentAction::Install,
                ComponentAction::Keep,
                ComponentAction::Deactivate,
            ]
        );
    }

    #[test]
    fn install_plan_should_separate_selected_system_packages() {
        let mut system = Component::new(
            "base-deps",
            "Base Dependencies",
            "Compilers, curl, git, tar, unzip",
            Category::SystemPackage,
            Group::System,
            None,
            &[],
        );
        system.action = ComponentAction::Install;

        let plan = InstallPlan::from_components(&[system]);

        assert_eq!(plan.system.len(), 1);
        assert!(plan.mise.is_empty());
        assert!(plan.deactivate_mise.is_empty());
        assert!(plan.configs.is_empty());
    }

    #[test]
    fn install_plan_should_only_clone_selected_components_per_phase() {
        let mut system = Component::new(
            "base-deps",
            "Base Dependencies",
            "Compilers, curl, git, tar, unzip",
            Category::SystemPackage,
            Group::System,
            None,
            &[],
        );
        system.action = ComponentAction::Install;

        let mut mise = Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Group::Languages,
            Some("rustc"),
            &["--version"],
        );
        mise.action = ComponentAction::Install;

        let mut config = Component::new(
            "config-fish",
            "Fish Configuration",
            "Aliases, colors, mise paths",
            Category::Config,
            Group::Configurations,
            None,
            &[],
        );
        config.action = ComponentAction::Keep;

        let plan = InstallPlan::from_components(&[system, mise, config]);

        assert_eq!(plan.system.len(), 1);
        assert_eq!(plan.mise.len(), 1);
        assert!(plan.deactivate_mise.is_empty());
        assert!(plan.configs.is_empty());
    }

    #[test]
    fn install_plan_should_preserve_mise_versions() {
        let mut mise = Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Group::Languages,
            Some("rustc"),
            &["--version"],
        );
        mise.action = ComponentAction::Install;
        mise.mise_version = Some("1.85.0".to_string());

        let plan = InstallPlan::from_components(&[mise]);

        assert_eq!(plan.mise[0].mise_version.as_deref(), Some("1.85.0"));
    }

    #[test]
    fn headless_config_path_should_parse_long_flag() {
        let args = vec![
            "devenv".to_string(),
            "--config".to_string(),
            "devenv.example.toml".to_string(),
        ];

        assert_eq!(
            headless_config_path(&args).expect("args should parse"),
            Some(PathBuf::from("devenv.example.toml"))
        );
    }

    #[test]
    fn headless_config_path_should_parse_equals_form() {
        let args = vec![
            "devenv".to_string(),
            "--config=devenv.example.toml".to_string(),
        ];

        assert_eq!(
            headless_config_path(&args).expect("args should parse"),
            Some(PathBuf::from("devenv.example.toml"))
        );
    }

    #[test]
    fn headless_config_path_should_reject_missing_path() {
        let args = vec!["devenv".to_string(), "--config".to_string()];

        assert!(headless_config_path(&args).is_err());
    }

    #[test]
    fn cli_helper_should_parse_help_flags() {
        for flag in ["--help", "-h"] {
            let args = vec!["devenv".to_string(), flag.to_string()];

            assert_eq!(cli_helper(&args), Some(CliHelper::Help));
        }
    }

    #[test]
    fn cli_helper_should_parse_version_flags() {
        for flag in ["--version", "-v"] {
            let args = vec!["devenv".to_string(), flag.to_string()];

            assert_eq!(cli_helper(&args), Some(CliHelper::Version));
        }
    }

    #[test]
    fn cli_helper_should_prefer_help_over_later_install_args() {
        let args = vec![
            "devenv".to_string(),
            "--help".to_string(),
            "--all".to_string(),
        ];

        assert_eq!(cli_helper(&args), Some(CliHelper::Help));
    }
}
