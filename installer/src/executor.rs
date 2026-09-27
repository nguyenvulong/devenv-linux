use crate::installer::{config, mise, system};
use crate::registry::{Component, ComponentOutcome, InstallPlan, config_needs_mise};
use crate::sys::check_command_exists;
use anyhow::{Result, bail};

pub const PHASES: [&str; 3] = ["System Packages", "Mise Tools", "Configurations"];

/// Execute every planned mutation in three phases, reporting each component
/// outcome independently. Shared by the TUI and both headless modes.
pub fn execute_plan<L>(
    plan: &InstallPlan,
    log: L,
    mut on_phase: impl FnMut(usize),
    mut on_outcome: impl FnMut(&str, ComponentOutcome),
) where
    L: Fn(&str) + Send + Clone + 'static,
{
    on_phase(0);
    log(&format!(">>> Phase 1: {}", PHASES[0]));
    let prerequisites: Vec<_> = plan.prerequisites.iter().map(|(p, _)| *p).collect();
    if !plan.system.is_empty() || !prerequisites.is_empty() {
        for (prerequisite, reason) in &plan.prerequisites {
            log(&format!(
                "Prerequisite: {} (for {reason})",
                prerequisite.label()
            ));
        }
        let components: Vec<&Component> = plan.system.iter().collect();
        let result = system::install_system_packages(&components, &prerequisites, log.clone());
        if let Err(error) = &result {
            log(&format!("[ERROR] System packages: {error}"));
        }
        for component in &plan.system {
            let outcome = match &result {
                Ok(()) => ComponentOutcome::Succeeded,
                Err(error) => ComponentOutcome::Failed(error.to_string()),
            };
            on_outcome(&component.id, outcome);
        }
    }

    on_phase(1);
    log(&format!("\n>>> Phase 2: {}", PHASES[1]));
    for component in &plan.deactivate_mise {
        match mise::deactivate_mise_tools(&[component], log.clone()) {
            Ok(()) => on_outcome(&component.id, ComponentOutcome::Deactivated),
            Err(error) => {
                log(&format!("[ERROR] deactivate {}: {error}", component.id));
                on_outcome(&component.id, ComponentOutcome::Failed(error.to_string()));
            }
        }
    }

    let mise_error = if plan.needs_mise_install() {
        prepare_mise(&log)
            .err()
            .map(|error| format!("mise prerequisite: {error}"))
    } else {
        None
    };
    if let Some(error) = &mise_error {
        log(&format!("[ERROR] {error}"));
    }

    let mut installed_tools = Vec::new();
    for component in &plan.mise {
        if let Some(error) = &mise_error {
            on_outcome(&component.id, ComponentOutcome::Failed(error.clone()));
            continue;
        }
        match mise::activate_mise_tools(&[component], log.clone()) {
            Ok(()) => {
                installed_tools.push(component.id.clone());
                on_outcome(&component.id, ComponentOutcome::Succeeded)
            }
            Err(error) => {
                log(&format!("[ERROR] mise tool {}: {error}", component.id));
                on_outcome(&component.id, ComponentOutcome::Failed(error.to_string()));
            }
        }
    }

    on_phase(2);
    log(&format!("\n>>> Phase 3: {}", PHASES[2]));

    // Shell setup is implicit: whenever mise tools are installed, mise and the
    // tools must be on PATH in bash (and fish), whatever else was selected.
    // It also implements the Bash Configuration component.
    if plan.needs_shell_setup() && mise_error.is_none() {
        let fish = plan.involves_fish() || check_command_exists("fish");
        log("Setting up mise on PATH for your shells...");
        let result = config::setup_shell_path(fish, log.clone());
        let bash_selected = plan.configs.iter().any(|c| c.id == "config-bash");
        match &result {
            Ok(outcome) if bash_selected => on_outcome("config-bash", config_outcome(*outcome)),
            Ok(_) => {}
            Err(error) => {
                log(&format!("[ERROR] shell setup: {error}"));
                let message = format!("installed, but adding mise to PATH failed: {error}");
                for id in &installed_tools {
                    on_outcome(id, ComponentOutcome::Failed(message.clone()));
                }
                if bash_selected {
                    on_outcome("config-bash", ComponentOutcome::Failed(error.to_string()));
                }
            }
        }
    }

    for component in &plan.configs {
        if component.id == "config-bash" && mise_error.is_none() {
            continue; // handled by the shell setup above
        }
        if let Some(error) = mise_error.as_ref().filter(|_| config_needs_mise(component)) {
            on_outcome(&component.id, ComponentOutcome::Failed(error.clone()));
            continue;
        }
        if component.id == "config-nvim" && !check_command_exists("git") {
            let error = "git is required to clone the LazyVim starter; \
                         install Build Tools or git first";
            log(&format!("[ERROR] config {}: {error}", component.id));
            on_outcome(&component.id, ComponentOutcome::Failed(error.to_string()));
            continue;
        }

        match config::setup_config(component, log.clone()) {
            Ok(outcome) => on_outcome(&component.id, config_outcome(outcome)),
            Err(error) => {
                log(&format!("[ERROR] config {}: {error}", component.id));
                on_outcome(&component.id, ComponentOutcome::Failed(error.to_string()));
            }
        }
    }

    on_phase(PHASES.len());
}

fn config_outcome(outcome: config::ConfigOutcome) -> ComponentOutcome {
    match outcome {
        config::ConfigOutcome::Changed => ComponentOutcome::Succeeded,
        config::ConfigOutcome::AlreadyConfigured => ComponentOutcome::AlreadyConfigured,
    }
}

/// What the user should do after an install, shown in the TUI summary and
/// printed by headless modes.
pub fn next_steps(plan: &InstallPlan) -> Vec<String> {
    let mut steps = Vec::new();
    if plan.needs_shell_setup() {
        steps.push(
            "Open a new terminal (or run `exec bash -l`) so mise and your tools are on PATH."
                .to_string(),
        );
    }
    if plan.involves_fish() {
        steps.push("Type `fish` to start Fish. Your login shell is unchanged.".to_string());
    }
    if plan.configs.iter().any(|c| c.id == "config-nvim") {
        steps.push("Run `nvim` once to let LazyVim install its plugins.".to_string());
    }
    steps
}

fn prepare_mise<L>(log: &L) -> Result<()>
where
    L: Fn(&str) + Send + Clone + 'static,
{
    if !mise::is_installed() && !check_command_exists("curl") {
        bail!("curl is required to bootstrap mise; install Build Tools or curl first");
    }
    mise::install_mise(log.clone())
}
