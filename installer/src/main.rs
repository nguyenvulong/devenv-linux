use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend, prelude::Backend};
use std::{
    collections::HashMap,
    error::Error,
    io::{self, IsTerminal, Write},
    panic::{self, AssertUnwindSafe},
    path::PathBuf,
    process::{Command, ExitCode},
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::Duration,
};

mod app;
mod executor;
mod headless_config;
mod installer;
mod manifest;
mod registry;
mod sys;
mod theme;
mod ui;

use app::{App, Screen};
use registry::{Component, ComponentAction, ComponentOutcome, InstallPlan};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let installer_all = std::env::var("INSTALLER_ALL").is_ok_and(|value| value == "1");
    let mode = match parse_args(&args, installer_all) {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("error: {error}");
            eprintln!("Run 'devenv --help' for usage.");
            return ExitCode::from(2);
        }
    };

    let result = match mode {
        Mode::Help => {
            print_help();
            Ok(())
        }
        Mode::Version => {
            print_version();
            Ok(())
        }
        Mode::All => run_headless(),
        Mode::Config(path) => run_headless_config(path),
        Mode::Interactive => run_interactive(),
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Mode {
    Help,
    Version,
    All,
    Config(PathBuf),
    Interactive,
}

/// Parse command-line arguments strictly. Help and version win over any other
/// argument; unknown arguments and conflicting modes are rejected.
fn parse_args(args: &[String], installer_all: bool) -> Result<Mode, String> {
    let args = args.get(1..).unwrap_or_default();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(Mode::Help);
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-v") {
        return Ok(Mode::Version);
    }

    let mut all = false;
    let mut config: Option<PathBuf> = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let path = if arg == "--all" {
            all = true;
            continue;
        } else if arg == "--config" || arg == "-c" {
            iter.next()
                .map(String::as_str)
                .ok_or_else(|| format!("{arg} requires a path"))?
        } else if let Some(path) = arg.strip_prefix("--config=") {
            path
        } else {
            return Err(format!("unknown argument: {arg}"));
        };

        if path.is_empty() {
            return Err("--config requires a path".to_string());
        }
        if config.replace(PathBuf::from(path)).is_some() {
            return Err("--config may only be given once".to_string());
        }
    }

    match (all, config) {
        (true, Some(_)) => Err("--all and --config cannot be combined".to_string()),
        (_, Some(path)) => Ok(Mode::Config(path)),
        (true, None) => Ok(Mode::All),
        (false, None) if installer_all => Ok(Mode::All),
        (false, None) => Ok(Mode::Interactive),
    }
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

Environment:
  INSTALLER_ALL=1        Same as --all

Without --all or --config, an interactive terminal is required.
",
        env!("CARGO_PKG_VERSION")
    );
}

fn print_version() {
    println!("devenv {}", env!("CARGO_PKG_VERSION"));
}

fn run_interactive() -> Result<(), Box<dyn Error>> {
    let ci = std::env::var("CI").is_ok_and(|value| value == "true");
    if ci || !io::stdout().is_terminal() {
        return Err("no interactive terminal detected; \
             use --all or --config <PATH> for non-interactive installs"
            .into());
    }

    install_panic_hook();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run_tui(&mut terminal);

    restore_terminal()?;
    terminal.show_cursor()?;
    res
}

fn run_tui<B: Backend>(terminal: &mut Terminal<B>) -> Result<(), Box<dyn Error>>
where
    <B as Backend>::Error: 'static,
{
    terminal.draw(ui::draw_loading)?;
    let mut app = App::new();
    run_app(terminal, &mut app)
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)
}

/// Restore the terminal before printing a panic from the TUI thread so the
/// message is readable and the shell is not left in raw mode.
fn install_panic_hook() {
    let default_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if thread::current().name() == Some("main") {
            let _ = restore_terminal();
        }
        default_hook(info);
    }));
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

    let install_plan = InstallPlan::for_environment(&components);
    if install_plan.needs_sudo() && !installer::system::can_install_packages() {
        println!(
            "Warning: sudo is not installed; system packages will fail. Run as root or install sudo."
        );
    } else if install_plan.needs_sudo() && !sys::is_root() {
        println!("Some components require elevated privileges (sudo).");
        let status = Command::new("sudo")
            .arg("-v")
            .status()
            .map_err(|e| format!("Failed to run sudo: {e}"))?;
        if !status.success() {
            return Err("sudo authentication failed".into());
        }
        start_sudo_keepalive();
    }

    let mut outcomes: HashMap<String, ComponentOutcome> = components
        .iter()
        .map(|component| (component.id.clone(), ComponentOutcome::Kept))
        .collect();

    executor::execute_plan(
        &install_plan,
        |msg: &str| println!("{msg}"),
        |_| {},
        |id, outcome| {
            outcomes.insert(id.to_string(), outcome);
        },
    );

    println!("\nInstallation report:");
    for component in &components {
        if let Some(outcome) = outcomes.get(&component.id) {
            println!("{}: {}", component.id, outcome_label(outcome));
        }
    }
    let steps = executor::next_steps(&install_plan);
    if !steps.is_empty() {
        println!("\nNext steps:");
        for step in steps {
            println!("  - {step}");
        }
    }
    let failures = outcomes
        .values()
        .filter(|outcome| matches!(outcome, ComponentOutcome::Failed(_)))
        .count();
    if failures > 0 {
        return Err(format!("{failures} component(s) failed to install").into());
    }
    println!("\n✅ All done!");
    Ok(())
}

fn outcome_label(outcome: &ComponentOutcome) -> String {
    match outcome {
        ComponentOutcome::Pending => "Pending".to_string(),
        ComponentOutcome::Succeeded => "Succeeded".to_string(),
        ComponentOutcome::Failed(error) => format!("Failed ({error})"),
        ComponentOutcome::AlreadyConfigured => "Already configured".to_string(),
        ComponentOutcome::Deactivated => "Deactivated".to_string(),
        ComponentOutcome::Kept => "Kept".to_string(),
    }
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

        // Always wait on input with a timeout so the loop never spins, even
        // while the installation thread is running.
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

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
                    let plan = InstallPlan::for_environment(&app.components);
                    if plan.needs_sudo() && !ensure_sudo_credentials_for_install()? {
                        continue;
                    }

                    app.prepare_installation();
                    app.screen = Screen::Installing;
                    spawn_installation(app);
                }
                _ => {}
            },
            Screen::Installing => {
                if key.code == KeyCode::Enter && app.install_done.load(Ordering::Acquire) {
                    app.screen = Screen::Report;
                }
            }
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

fn ensure_sudo_credentials_for_install() -> Result<bool, Box<dyn Error>> {
    // Without sudo the system phase fails with a clear error (shown in review).
    if sys::is_root() || !installer::system::can_install_packages() {
        return Ok(true);
    }
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
            let Ok(status) = Command::new("sudo").args(["-n", "-v"]).status() else {
                break;
            };
            if !status.success() {
                break;
            }
        }
    });
}

fn suspend_tui() -> Result<(), Box<dyn Error>> {
    restore_terminal()?;
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
    let install_plan = InstallPlan::for_environment(&app.components);

    thread::spawn(move || {
        let log = {
            let logs = Arc::clone(&logs);
            move |msg: &str| push_log(&logs, msg)
        };
        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            executor::execute_plan(
                &install_plan,
                log,
                |phase| install_index.store(phase, Ordering::Relaxed),
                |id, outcome| set_outcome(&outcomes, id, outcome),
            );
        }));

        if result.is_err() {
            push_log(&logs, "[ERROR] The installer thread panicked.");
        }
        push_log(
            &logs,
            "\nInstallation finished. Press Enter to view the summary.",
        );
        done_flag.store(true, Ordering::Release);
    });
}

fn push_log(logs: &Arc<Mutex<Vec<String>>>, message: impl Into<String>) {
    if let Ok(mut guard) = logs.lock() {
        guard.extend(log_lines(&message.into()));
    }
}

/// Split a message into display lines and drop terminal control sequences.
/// Tool output (e.g. mise) may contain ANSI colors, and raw escapes or
/// newlines written into a ratatui buffer corrupt the TUI layout.
fn log_lines(message: &str) -> Vec<String> {
    message
        .split('\n')
        .map(|line| strip_control(line.rsplit('\r').next().unwrap_or_default()))
        .collect()
}

fn strip_control(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\x1b' => match chars.next() {
                // CSI: ESC [ parameters... final byte in @..~
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                // OSC: ESC ] ... terminated by BEL or ESC \
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\t' => out.push_str("    "),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn set_outcome(
    outcomes: &Arc<Mutex<HashMap<String, ComponentOutcome>>>,
    component_id: &str,
    outcome: ComponentOutcome,
) {
    if let Ok(mut outcomes) = outcomes.lock() {
        outcomes.insert(component_id.to_string(), outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::{InstallPlan, Mode, handle_selection_action_key, log_lines, parse_args};

    #[test]
    fn log_lines_should_strip_escapes_and_split_newlines() {
        assert_eq!(
            log_lines("\n>>> Phase 2: Mise Tools"),
            vec!["".to_string(), ">>> Phase 2: Mise Tools".to_string()]
        );
        assert_eq!(
            log_lines("\x1b[2mmise\x1b[0m \x1b[38;5;11m⇢\x1b[0m node@26\tok"),
            vec!["mise ⇢ node@26    ok".to_string()]
        );
        assert_eq!(
            log_lines("\x1b]8;;https://x\x07link\x1b]8;;\x1b\\ 50%\r100%"),
            vec!["100%".to_string()]
        );
    }
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

    fn args(values: &[&str]) -> Vec<String> {
        std::iter::once("devenv")
            .chain(values.iter().copied())
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn parse_args_should_parse_config_forms() {
        for values in [
            &["--config", "devenv.example.toml"][..],
            &["-c", "devenv.example.toml"],
            &["--config=devenv.example.toml"],
        ] {
            assert_eq!(
                parse_args(&args(values), false),
                Ok(Mode::Config(PathBuf::from("devenv.example.toml")))
            );
        }
    }

    #[test]
    fn parse_args_should_reject_missing_config_path() {
        assert!(parse_args(&args(&["--config"]), false).is_err());
        assert!(parse_args(&args(&["--config="]), false).is_err());
    }

    #[test]
    fn parse_args_should_reject_unknown_and_conflicting_arguments() {
        for values in [
            &["--alll"][..],
            &["install"],
            &["--all", "--config", "a.toml"],
            &["-c", "a.toml", "-c", "b.toml"],
        ] {
            assert!(parse_args(&args(values), false).is_err(), "{values:?}");
        }
    }

    #[test]
    fn parse_args_should_select_modes() {
        assert_eq!(parse_args(&args(&[]), false), Ok(Mode::Interactive));
        assert_eq!(parse_args(&args(&["--all"]), false), Ok(Mode::All));
        assert_eq!(parse_args(&args(&[]), true), Ok(Mode::All));
        assert_eq!(
            parse_args(&args(&["-c", "a.toml"]), true),
            Ok(Mode::Config(PathBuf::from("a.toml")))
        );
    }

    #[test]
    fn parse_args_should_parse_help_and_version_flags() {
        for flag in ["--help", "-h"] {
            assert_eq!(parse_args(&args(&[flag]), false), Ok(Mode::Help));
        }
        for flag in ["--version", "-v"] {
            assert_eq!(parse_args(&args(&[flag]), false), Ok(Mode::Version));
        }
    }

    #[test]
    fn parse_args_should_prefer_help_over_other_args() {
        assert_eq!(
            parse_args(&args(&["--all", "--bogus", "--help"]), false),
            Ok(Mode::Help)
        );
    }
}
