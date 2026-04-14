use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, prelude::Backend, Terminal};
use std::{
    error::Error,
    io,
    io::Write,
    process::Command,
    sync::{Arc, Mutex, atomic::Ordering},
    thread,
    time::Duration,
};

mod app;
mod installer;
mod manifest;
mod registry;
mod sys;
mod theme;
mod ui;

use app::{App, Screen};
use registry::{Category, Component, SelectionState};

fn main() -> Result<(), Box<dyn Error>> {
    let headless = std::env::args().any(|a| a == "--all")
        || std::env::var("CI").map(|v| v == "true").unwrap_or(false)
        || std::env::var("INSTALLER_ALL")
            .map(|v| v == "1")
            .unwrap_or(false);

    if headless {
        return run_headless();
    }

    if !sys::check_command_exists("mise") {
        let mise_path = installer::mise::mise_bin();
        if mise_path == "mise" {
            println!("Setting up package registry (installing mise)...");
            let _ = installer::mise::install_mise(|_| {});
        }
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

fn run_headless() -> Result<(), Box<dyn Error>> {
    println!("==> devenv-linux headless installer (--all mode)");
    println!();

    let mut components = registry::get_all_components();
    for c in &mut components {
        c.state = registry::SelectionState::Selected;
    }

    let needs_sudo = components
        .iter()
        .any(|c| matches!(c.category, registry::Category::SystemPackage));

    if needs_sudo {
        println!("Some components require elevated privileges (sudo).");
        let status = Command::new("sudo").arg("-v").status()?;
        if !status.success() {
            eprintln!("sudo authentication failed. Aborting.");
            std::process::exit(1);
        }
        start_sudo_keepalive();
    }

    println!(">>> Phase 1: System Packages");
    let sys_comps: Vec<&registry::Component> = components
        .iter()
        .filter(|c| matches!(c.category, registry::Category::SystemPackage))
        .collect();
    if let Err(e) =
        installer::system::install_system_packages(&sys_comps, |msg| println!("{}", msg))
    {
        eprintln!("[ERROR] System packages: {}", e);
    }

    println!("\n>>> Phase 2: Mise Tools");
    let mise_comps: Vec<&registry::Component> = components
        .iter()
        .filter(|c| matches!(c.category, registry::Category::Mise(_)))
        .collect();
    if !mise_comps.is_empty() {
        match installer::mise::install_mise(|msg| println!("{}", msg)) {
            Err(e) => eprintln!("[ERROR] mise install: {}", e),
            Ok(()) => {
                if let Err(e) =
                    installer::mise::activate_mise_tools(&mise_comps, |msg| println!("{}", msg))
                {
                    eprintln!("[ERROR] mise tools: {}", e);
                }
            }
        }
    }

    println!("\n>>> Phase 3: Configurations");
    let cfg_comps: Vec<&registry::Component> = components
        .iter()
        .filter(|c| matches!(c.category, registry::Category::Config))
        .collect();
    for cfg in cfg_comps {
        if let Err(e) = installer::config::setup_config(cfg, |msg| println!("{}", msg)) {
            eprintln!("[ERROR] config {}: {}", cfg.id, e);
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
                for c in &mut app.components {
                    if c.state == registry::SelectionState::Selected {
                        c.status = registry::InstallStatus::Installed("Done".to_string());
                    }
                }
                app.screen = Screen::Report;
            }
            continue;
        }

        if event::poll(Duration::from_millis(100))? && let Event::Key(key) = event::read()? {
            match app.screen {
                Screen::Selection => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    KeyCode::Char(' ') => app.toggle_selection(),
                    KeyCode::Char('a') => {
                        for c in &mut app.components {
                            c.state = registry::SelectionState::Selected;
                        }
                    }
                    KeyCode::Char('n') => {
                        for c in &mut app.components {
                            c.state = registry::SelectionState::Unselected;
                        }
                    }
                    KeyCode::Char('/') => {
                        app.search_query.clear();
                        app.update_search();
                        app.screen = Screen::Search;
                    }
                    KeyCode::Enter => {
                        if app.has_selected_system_packages()
                            && !ensure_sudo_credentials_for_install()?
                        {
                            continue;
                        }

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

fn ensure_sudo_credentials_for_install() -> Result<bool, Box<dyn Error>> {
    if has_cached_sudo_credentials()? {
        start_sudo_keepalive();
        return Ok(true);
    }

    suspend_tui()?;

    println!("System packages are selected and require sudo.");
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
    thread::spawn(|| loop {
        thread::sleep(Duration::from_secs(50));
        let Ok(status) = Command::new("sudo").args(["-n", "true"]).status() else {
            break;
        };

        if !status.success() {
            break;
        }

        let _ = Command::new("sudo").arg("-v").output();
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
        if let Err(e) =
            installer::system::install_system_packages(&sys_comps, make_log(logs.clone()))
        {
            push_log(&logs, format!("[ERROR] System packages: {}", e));
        }

        install_index.store(1, Ordering::Relaxed);
        push_log(&logs, "\n>>> Phase 2: Mise Tools");

        let mise_comps: Vec<&Component> = install_plan.mise.iter().collect();
        if !mise_comps.is_empty() {
            match installer::mise::install_mise(make_log(logs.clone())) {
                Err(e) => push_log(&logs, format!("[ERROR] mise install: {}", e)),
                Ok(()) => {
                    if let Err(e) =
                        installer::mise::activate_mise_tools(&mise_comps, make_log(logs.clone()))
                    {
                        push_log(&logs, format!("[ERROR] mise tools: {}", e));
                    }
                }
            }
        }

        install_index.store(2, Ordering::Relaxed);
        push_log(&logs, "\n>>> Phase 3: Configurations");

        let cfg_comps: Vec<&Component> = install_plan.configs.iter().collect();
        for cfg in cfg_comps {
            if let Err(e) = installer::config::setup_config(cfg, make_log(logs.clone())) {
                push_log(&logs, format!("[ERROR] config {}: {}", cfg.id, e));
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

struct InstallPlan {
    system: Vec<Component>,
    mise: Vec<Component>,
    configs: Vec<Component>,
}

impl InstallPlan {
    fn from_components(components: &[Component]) -> Self {
        Self {
            system: collect_selected_components(components, |category| {
                matches!(category, Category::SystemPackage)
            }),
            mise: collect_selected_components(components, |category| {
                matches!(category, Category::Mise(_))
            }),
            configs: collect_selected_components(components, |category| {
                matches!(category, Category::Config)
            }),
        }
    }
}

fn collect_selected_components(
    components: &[Component],
    predicate: impl Fn(&Category) -> bool,
) -> Vec<Component> {
    components
        .iter()
        .filter(|component| {
            component.state == SelectionState::Selected && predicate(&component.category)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::InstallPlan;
    use crate::registry::{Category, Component, Group, SelectionState};

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
        system.state = SelectionState::Selected;

        let plan = InstallPlan::from_components(&[system]);

        assert_eq!(plan.system.len(), 1);
        assert!(plan.mise.is_empty());
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
        system.state = SelectionState::Selected;

        let mut mise = Component::new(
            "rust",
            "Rust",
            "Rust programming language",
            Category::Mise("rust".to_string()),
            Group::Languages,
            Some("rustc"),
            &["--version"],
        );
        mise.state = SelectionState::Selected;

        let mut config = Component::new(
            "config-fish",
            "Fish Configuration",
            "Aliases, colors, mise paths",
            Category::Config,
            Group::Configurations,
            None,
            &[],
        );
        config.state = SelectionState::KeepAsIs;

        let plan = InstallPlan::from_components(&[system, mise, config]);

        assert_eq!(plan.system.len(), 1);
        assert_eq!(plan.mise.len(), 1);
        assert!(plan.configs.is_empty());
    }
}
