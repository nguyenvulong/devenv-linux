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

    for component in &plan.mise {
        if let Some(error) = &mise_error {
            on_outcome(&component.id, ComponentOutcome::Failed(error.clone()));
            continue;
        }
        match mise::activate_mise_tools(&[component], log.clone()) {
            Ok(()) => on_outcome(&component.id, ComponentOutcome::Succeeded),
            Err(error) => {
                log(&format!("[ERROR] mise tool {}: {error}", component.id));
                on_outcome(&component.id, ComponentOutcome::Failed(error.to_string()));
            }
        }
    }

    on_phase(2);
    log(&format!("\n>>> Phase 3: {}", PHASES[2]));
    for component in &plan.configs {
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
            Ok(config::ConfigOutcome::Changed) => {
                on_outcome(&component.id, ComponentOutcome::Succeeded)
            }
            Ok(config::ConfigOutcome::AlreadyConfigured) => {
                on_outcome(&component.id, ComponentOutcome::AlreadyConfigured)
            }
            Err(error) => {
                log(&format!("[ERROR] config {}: {error}", component.id));
                on_outcome(&component.id, ComponentOutcome::Failed(error.to_string()));
            }
        }
    }

    on_phase(PHASES.len());
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
