use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, prelude::Backend, Terminal};
use std::{
    error::Error,
    io,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

mod app;
mod installer;
mod manifest;
mod registry;
mod sys;
mod ui;

use app::{App, Screen};

fn main() -> Result<(), Box<dyn Error>> {
    // ── Non-interactive / headless mode ──────────────────────────────────────
    // Activated by: `--all` flag, `CI=true`, or `INSTALLER_ALL=1` env var.
    // Skips the TUI entirely — useful for CI pipelines and scripted installs.
    let headless = std::env::args().any(|a| a == "--all")
        || std::env::var("CI").map(|v| v == "true").unwrap_or(false)
        || std::env::var("INSTALLER_ALL").map(|v| v == "1").unwrap_or(false);

    if headless {
        return run_headless();
    }

    // ── Pre-flight: cache sudo credentials BEFORE entering raw mode ──────────
    // Check if any SystemPackage components would be selected (i.e., need sudo).
    // We do a quick pre-scan without the TUI to decide if we need to prompt.
    let needs_sudo = registry::get_all_components()
        .iter()
        .any(|c| matches!(c.category, registry::Category::SystemPackage));

    if needs_sudo {
        println!("Some components require elevated privileges (sudo).");
        println!("Please enter your sudo password now so installation won't block later.");
        println!();

        // Run `sudo -v` in the *normal* terminal (before raw mode) to:
        //   1. Show the password prompt visibly to the user.
        //   2. Cache credentials for the session.
        let status = Command::new("sudo")
            .arg("-v")
            .status()
            .map_err(|e| format!("Failed to run sudo: {}", e))?;

        if !status.success() {
            eprintln!("sudo authentication failed. Aborting.");
            std::process::exit(1);
        }

        // Keep the credential cache alive in a background thread by refreshing
        // every 50 seconds (default sudo timeout is 5-15 minutes, but this is
        // an extra safety net for long installs).
        thread::spawn(|| loop {
            thread::sleep(Duration::from_secs(50));
            let _ = Command::new("sudo").arg("-v").output();
        });
    }

    // ── Enter the TUI ────────────────────────────────────────────────────────
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
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

/// Headless installation: runs all phases without the TUI.
/// All components are force-selected; logs are printed directly to stdout.
fn run_headless() -> Result<(), Box<dyn Error>> {
    println!("==> devenv-linux headless installer (--all mode)");
    println!();

    let mut components = registry::get_all_components();

    // Force-select everything.
    for c in &mut components {
        c.state = registry::SelectionState::Selected;
    }

    // ── Phase 1: System packages ──────────────────────────────────────────────
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

    // ── Phase 2: mise tools ───────────────────────────────────────────────────
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

    // ── Phase 3: Configurations ───────────────────────────────────────────────
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

    println!("\n\u{2705} All done!");
    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), Box<dyn Error>>
where
    <B as Backend>::Error: 'static,
{
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        if app.should_quit {
            return Ok(());
        }

        // ── Installing screen: just keep redrawing and check for completion ──
        if app.screen == Screen::Installing {
            // Poll for 'q' to cancel (future feature) but don't block.
            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        // Allow cancel only before installation starts (guard in future).
                    }
                }
            }

            // Transition to Report once the worker thread signals completion.
            let done = *app.install_done.lock().unwrap();
            if done {
                // Refresh statuses based on what's on PATH now.
                for c in &mut app.components {
                    if c.state == registry::SelectionState::Selected {
                        c.status = registry::InstallStatus::Installed("Done".to_string());
                    }
                }
                app.screen = Screen::Report;
            }
            continue;
        }

        // ── Event handling for other screens ─────────────────────────────────
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
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
                            app.screen = Screen::Installing;
                            spawn_installation(app);
                        }
                        _ => {}
                    },
                    Screen::Search => match key.code {
                        KeyCode::Esc => app.screen = Screen::Selection,
                        // In search mode, keep plain character keys for query input.
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
                        KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => {
                            app.should_quit = true
                        }
                        _ => {}
                    },
                    _ => {}
                }
            }
        }
    }
}

/// Spawns a background thread that runs all installation phases.
/// The thread writes log lines into `app.logs` and sets `app.install_done` when finished.
fn spawn_installation(app: &mut App) {
    let logs = app.logs.clone();
    let done_flag = app.install_done.clone();
    let install_index = app.install_index.clone();

    // Clone the component list so the thread owns it.
    let components = app.components.clone();

    // Build a log closure that pushes into the shared Arc<Mutex<Vec<String>>>.
    let make_log = move |logs: Arc<Mutex<Vec<String>>>| {
        move |msg: &str| {
            if let Ok(mut guard) = logs.lock() {
                guard.push(msg.to_string());
            }
        }
    };

    thread::spawn(move || {
        // ── Phase 1: System packages (sudo credentials already cached) ────────
        *install_index.lock().unwrap() = 0;
        {
            let mut g = logs.lock().unwrap();
            g.push(">>> Phase 1: System Packages".to_string());
        }

        let sys_comps: Vec<&registry::Component> = components
            .iter()
            .filter(|c| {
                c.state == registry::SelectionState::Selected
                    && matches!(c.category, registry::Category::SystemPackage)
            })
            .collect();

        if let Err(e) =
            installer::system::install_system_packages(&sys_comps, make_log(logs.clone()))
        {
            if let Ok(mut g) = logs.lock() {
                g.push(format!("[ERROR] System packages: {}", e));
            }
        }

        // ── Phase 2: mise tools ───────────────────────────────────────────────
        *install_index.lock().unwrap() = 1;
        {
            let mut g = logs.lock().unwrap();
            g.push("\n>>> Phase 2: Mise Tools".to_string());
        }

        let mise_comps: Vec<&registry::Component> = components
            .iter()
            .filter(|c| {
                c.state == registry::SelectionState::Selected
                    && matches!(c.category, registry::Category::Mise(_))
            })
            .collect();

        if !mise_comps.is_empty() {
            match installer::mise::install_mise(make_log(logs.clone())) {
                Err(e) => {
                    if let Ok(mut g) = logs.lock() {
                        g.push(format!("[ERROR] mise install: {}", e));
                    }
                }
                Ok(()) => {
                    if let Err(e) = installer::mise::activate_mise_tools(
                        &mise_comps,
                        make_log(logs.clone()),
                    ) {
                        if let Ok(mut g) = logs.lock() {
                            g.push(format!("[ERROR] mise tools: {}", e));
                        }
                    }
                }
            }
        }

        // ── Phase 3: Configurations ───────────────────────────────────────────
        *install_index.lock().unwrap() = 2;
        {
            let mut g = logs.lock().unwrap();
            g.push("\n>>> Phase 3: Configurations".to_string());
        }

        let cfg_comps: Vec<&registry::Component> = components
            .iter()
            .filter(|c| {
                c.state == registry::SelectionState::Selected
                    && matches!(c.category, registry::Category::Config)
            })
            .collect();

        for cfg in cfg_comps {
            if let Err(e) = installer::config::setup_config(cfg, make_log(logs.clone())) {
                if let Ok(mut g) = logs.lock() {
                    g.push(format!("[ERROR] config {}: {}", cfg.id, e));
                }
            }
        }

        // ── Done ──────────────────────────────────────────────────────────────
        {
            let mut g = logs.lock().unwrap();
            g.push("\n\u{2705} All done! Press Enter to view the summary.".to_string());
        }
        *done_flag.lock().unwrap() = true;
    });
}
