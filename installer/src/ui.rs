use crate::app::{App, Screen};
use crate::registry::{ComponentAction, ComponentOutcome, Group, InstallPlan, ObservedState};
use crate::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Gauge, List, ListItem, ListState, Paragraph, Row, Table, TableState},
};
use std::sync::atomic::Ordering;

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Selection => draw_selection(f, app),
        Screen::Review => draw_review(f, app),
        Screen::Installing => draw_installing(f, app),
        Screen::Report => draw_report(f, app),
        Screen::Search => draw_search(f, app),
    }
}

fn draw_selection(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(5),
        ])
        .split(f.area());

    let title = Paragraph::new(" devenv-linux Interactive Installer ")
        .style(theme::title_style())
        .block(theme::default_block());
    f.render_widget(title, chunks[0]);

    // ── Build list items with group section headers ──────────────────────────
    //
    // Strategy (Option A): app.cursor indexes into app.components directly.
    // We build a display list that interleaves non-selectable header rows with
    // component rows. A separate vec maps display rows back to component indices.

    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[1]);

    let mut items: Vec<ListItem> = Vec::new();
    // Maps display-row index → component index (None = header row)
    let mut row_to_component: Vec<Option<usize>> = Vec::new();

    let mut prev_group: Option<Group> = None;

    for (comp_idx, c) in app.components.iter().enumerate() {
        // Insert a section header when the group changes
        if prev_group != Some(c.group) {
            prev_group = Some(c.group);
            let header = ListItem::new(Line::from(vec![Span::styled(
                format!(" {} ", c.group.label()),
                theme::header_style(),
            )]));
            items.push(header);
            row_to_component.push(None);
        }

        let prefix = match c.action {
            ComponentAction::Keep => "[=] ",
            ComponentAction::Install => "[+] ",
            ComponentAction::Deactivate => "[-] ",
        };
        let prefix_style = match c.action {
            ComponentAction::Keep => theme::selection_unselected_style(),
            ComponentAction::Install => theme::selection_selected_style(),
            ComponentAction::Deactivate => theme::selection_uninstall_style(),
        };

        // Simplified list line (description goes to details pane)
        let line = Line::from(vec![
            Span::raw("  "), // indent under header
            Span::styled(prefix, prefix_style),
            Span::styled(&c.name, Style::default().add_modifier(Modifier::BOLD)),
        ]);

        // Highlight the row if this component is the current cursor position
        let mut item = ListItem::new(line);
        if comp_idx == app.cursor {
            item = item.style(theme::item_highlight_style());
        }
        items.push(item);
        row_to_component.push(Some(comp_idx));
    }

    let list = List::new(items).block(theme::default_block().title(" Tools & Configurations "));

    // Point ratatui's scroll offset at the display row for app.cursor
    let display_row = row_to_component
        .iter()
        .position(|r| *r == Some(app.cursor))
        .unwrap_or(0);
    let mut state = ListState::default();
    state.select(Some(display_row));
    f.render_stateful_widget(list, main_chunks[0], &mut state);

    // ── Build details pane ───────────────────────────────────────────────────
    if let Some(c) = app.components.get(app.cursor) {
        let mut details_text = vec![
            Line::from(vec![
                Span::styled("Name: ", theme::shortcut_key_style()),
                Span::styled(&c.name, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::styled("Group: ", theme::shortcut_key_style()),
                Span::raw(c.group.label()),
            ]),
            Line::from(""),
            Line::from(Span::styled("Description:", theme::shortcut_key_style())),
            Line::from(Span::raw(&c.description)),
            Line::from(""),
        ];

        let type_label = match &c.category {
            crate::registry::Category::Mise(tool) => format!("Mise Tool ({})", tool),
            crate::registry::Category::SystemPackage => {
                "System Package (apt/pacman/dnf)".to_string()
            }
            crate::registry::Category::Config => "Configuration / Bash Script".to_string(),
        };
        details_text.push(Line::from(vec![
            Span::styled("Type: ", theme::shortcut_key_style()),
            Span::raw(type_label),
        ]));

        if let Some(cmd) = &c.check_command {
            details_text.push(Line::from(vec![
                Span::styled("Check Command: ", theme::shortcut_key_style()),
                Span::raw(format!("{} {:?}", cmd, c.check_args.join(" "))),
            ]));
        }

        let status_color = match &c.observed {
            ObservedState::Missing => theme::COLOR_MUTED,
            ObservedState::PathDetected(_)
            | ObservedState::ExistingConfig
            | ObservedState::MiseGlobal { .. } => theme::COLOR_SUCCESS,
            ObservedState::Unknown => theme::COLOR_WARNING,
        };
        details_text.push(Line::from(vec![
            Span::styled("Observed: ", theme::shortcut_key_style()),
            Span::styled(c.observed_label(), Style::default().fg(status_color)),
        ]));
        details_text.push(Line::from(vec![
            Span::styled("Action: ", theme::shortcut_key_style()),
            Span::styled(
                c.action_label(),
                Style::default().fg(match c.action {
                    ComponentAction::Keep => theme::COLOR_MUTED,
                    ComponentAction::Install => theme::COLOR_SUCCESS,
                    ComponentAction::Deactivate => theme::COLOR_ERROR,
                }),
            ),
        ]));

        let details = Paragraph::new(details_text).block(theme::default_block().title(" Details "));
        f.render_widget(details, main_chunks[1]);
    }

    // ── Shortcut guide ───────────────────────────────────────────────────────
    let help_text = vec![
        Line::from(vec![
            Span::styled(" Navigate ", theme::shortcut_key_style()),
            Span::raw("↑/↓  j/k   "),
            Span::styled(" Install ", theme::success_text_style()),
            Span::raw("i   "),
            Span::styled(" Keep ", theme::shortcut_key_style()),
            Span::raw("u   "),
            Span::styled(" Deactivate ", theme::selection_uninstall_style()),
            Span::raw("d   "),
            Span::styled(" Toggle ", theme::shortcut_key_style()),
            Span::raw("<Space>"),
        ]),
        Line::from(vec![
            Span::styled(" Install all ", theme::success_text_style()),
            Span::raw("a   "),
            Span::styled(" Keep all ", theme::shortcut_key_style()),
            Span::raw("n   "),
            Span::styled(
                " Search ",
                theme::shortcut_action_style(theme::COLOR_HIGHLIGHT),
            ),
            Span::raw("/   "),
            Span::styled(" Review ", theme::success_text_style()),
            Span::raw("<Enter>   "),
            Span::styled(" Quit ", theme::shortcut_action_style(theme::COLOR_ERROR)),
            Span::raw("q"),
        ]),
        Line::from(app.notice.clone().unwrap_or_else(|| {
            " [=] Keep   [+] Install/update/reinstall   [-] Deactivate".to_string()
        })),
    ];

    let help = Paragraph::new(help_text).block(theme::default_block().title(" Shortcuts "));
    f.render_widget(help, chunks[2]);
}

fn draw_review(f: &mut Frame, app: &App) {
    let plan = InstallPlan::from_components(&app.components);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(5),
            Constraint::Length(3),
        ])
        .split(f.area());

    let title = Paragraph::new(" Review planned changes ")
        .style(theme::title_style())
        .block(theme::default_block());
    f.render_widget(title, chunks[0]);

    let rows = app
        .components
        .iter()
        .filter(|component| component.action != ComponentAction::Keep)
        .map(|component| {
            let color = match component.action {
                ComponentAction::Install => theme::COLOR_SUCCESS,
                ComponentAction::Deactivate => theme::COLOR_ERROR,
                ComponentAction::Keep => theme::COLOR_MUTED,
            };
            Row::new(vec![
                Span::styled(
                    component.name.clone(),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::styled(component.action_label(), Style::default().fg(color)),
                Span::raw(component.observed_label()),
            ])
        })
        .collect::<Vec<Row>>();

    let table = Table::new(
        rows,
        [
            Constraint::Length(25),
            Constraint::Length(18),
            Constraint::Min(30),
        ],
    )
    .header(
        Row::new(vec!["Component", "Planned action", "Current state"])
            .style(Style::default().add_modifier(Modifier::BOLD))
            .bottom_margin(1),
    )
    .block(theme::default_block().title(format!(
        " {} mutation(s) | {} kept ",
        plan.mutation_count(),
        plan.kept_count
    )))
    .column_spacing(2);
    f.render_widget(table, chunks[1]);

    let mut notes = Vec::new();
    if plan.needs_mise_install() {
        notes.push(Line::from(vec![
            Span::styled("Prerequisite: ", theme::shortcut_key_style()),
            Span::raw("mise will be installed if it is missing."),
        ]));
    }
    if plan.needs_sudo() {
        notes.push(Line::from(vec![
            Span::styled("Privilege: ", theme::shortcut_key_style()),
            Span::raw("sudo authentication follows confirmation."),
        ]));
    }
    if plan.replaces_existing_nvim_config() {
        notes.push(Line::from(Span::styled(
            "Warning: the existing Neovim config will be replaced after staging and backed up.",
            Style::default().fg(theme::COLOR_WARNING),
        )));
    }
    if notes.is_empty() {
        notes.push(Line::from(
            "No implicit prerequisites or replacement warnings.",
        ));
    }
    f.render_widget(
        Paragraph::new(notes).block(theme::default_block().title(" Notes ")),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(" <Enter> Confirm and execute    <Esc> Back to selection ")
            .block(theme::default_block()),
        chunks[3],
    );
}

fn draw_installing(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10)])
        .split(f.area());

    let phase = app.install_index.load(Ordering::Relaxed);
    const PHASES: f64 = 3.0;
    let phase_labels = ["System Packages", "Mise Tools", "Configurations"];
    let label = phase_labels.get(phase).copied().unwrap_or("Finishing…");
    let progress = ((phase as f64) / PHASES).clamp(0.0, 1.0);
    let gauge = Gauge::default()
        .block(theme::default_block().title(format!(
            " Installing: {} ({}/{}) ",
            label,
            phase + 1,
            PHASES as usize
        )))
        .gauge_style(
            Style::default()
                .fg(theme::COLOR_SUCCESS)
                .bg(theme::COLOR_MUTED),
        )
        .ratio(progress);
    f.render_widget(gauge, chunks[0]);

    let display_logs = app
        .logs
        .lock()
        .map(|logs| {
            logs.iter()
                .rev()
                .take(f.area().height as usize - 5)
                .rev()
                .cloned()
                .collect::<Vec<String>>()
        })
        .unwrap_or_default();

    // Alternate row colors for logs to improve readability
    let mut log_lines: Vec<Line> = Vec::new();
    for (i, log) in display_logs.iter().enumerate() {
        let style = if log.contains("ERROR") || log.contains("FAILED") || log.contains("stderr") {
            Style::default().fg(theme::COLOR_ERROR)
        } else if log.starts_with("==>") || log.contains("INFO") || log.contains("SUCCESS") {
            Style::default().fg(theme::COLOR_SUCCESS)
        } else if log.contains("WARN") {
            Style::default().fg(theme::COLOR_WARNING)
        } else if i % 2 == 0 {
            Style::default() // normal text
        } else {
            Style::default().fg(theme::COLOR_INFO) // alternating text
        };

        log_lines.push(Line::from(Span::styled(log, style)));
    }

    let p = Paragraph::new(log_lines).block(theme::default_block().title(" Log Stream "));
    f.render_widget(p, chunks[1]);
}

fn draw_report(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    let header_text = Paragraph::new(Span::styled(
        " Execution complete ",
        theme::success_text_style(),
    ))
    .block(theme::default_block());
    f.render_widget(header_text, chunks[0]);

    let outcomes = app
        .outcomes
        .lock()
        .map(|outcomes| outcomes.clone())
        .unwrap_or_default();
    let mut succeeded_count = 0;
    let mut failed_count = 0;
    let mut kept_count = 0;
    let mut rows: Vec<Row> = Vec::new();

    for component in &app.components {
        let outcome = outcomes
            .get(&component.id)
            .cloned()
            .unwrap_or(ComponentOutcome::Pending);
        let (status, color) = match outcome {
            ComponentOutcome::Succeeded => {
                succeeded_count += 1;
                ("Succeeded".to_string(), theme::COLOR_SUCCESS)
            }
            ComponentOutcome::Failed(error) => {
                failed_count += 1;
                (format!("Failed: {error}"), theme::COLOR_ERROR)
            }
            ComponentOutcome::AlreadyConfigured => {
                succeeded_count += 1;
                ("Already configured".to_string(), theme::COLOR_SUCCESS)
            }
            ComponentOutcome::Deactivated => {
                succeeded_count += 1;
                ("Deactivated".to_string(), theme::COLOR_SUCCESS)
            }
            ComponentOutcome::Kept => {
                kept_count += 1;
                ("Kept".to_string(), theme::COLOR_MUTED)
            }
            ComponentOutcome::Pending => {
                failed_count += 1;
                (
                    "Failed: no outcome recorded".to_string(),
                    theme::COLOR_WARNING,
                )
            }
        };

        rows.push(Row::new(vec![
            Span::styled(
                component.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(component.action_label(), Style::default().fg(color)),
            Span::styled(status, Style::default().fg(color)),
        ]));
    }

    let summary_title = format!(
        " Summary [ {succeeded_count} succeeded | {failed_count} failed | {kept_count} kept ] "
    );

    let table_header = Row::new(vec!["Component", "Requested action", "Outcome"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let table = Table::new(
        rows,
        [
            Constraint::Length(25),
            Constraint::Length(18),
            Constraint::Min(30),
        ],
    )
    .header(table_header)
    .block(theme::default_block().title(summary_title))
    .column_spacing(2);

    f.render_widget(table, chunks[1]);

    let footer = Paragraph::new(" Press 'q' or <Enter> to exit ")
        .style(Style::default().fg(theme::COLOR_MUTED))
        .block(theme::default_block());
    f.render_widget(footer, chunks[2]);
}

fn draw_search(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title
            Constraint::Length(3), // search input
            Constraint::Min(5),    // results
            Constraint::Length(3), // help bar
        ])
        .split(f.area());

    // Title
    let title = Paragraph::new(" Search Mise Registry ")
        .style(theme::shortcut_action_style(theme::COLOR_HIGHLIGHT))
        .block(theme::default_block());
    f.render_widget(title, chunks[0]);

    // Search input
    let input_text = format!(" {} ", app.search_query);
    let input = Paragraph::new(Line::from(vec![
        Span::styled("Search: ", theme::shortcut_key_style()),
        Span::raw(&input_text),
        Span::styled("█", Style::default().fg(theme::COLOR_INFO)),
    ]))
    .block(theme::default_block());
    f.render_widget(input, chunks[1]);

    // Results list
    let results = &app.search_results;

    let header = Row::new(vec!["Tool Name", "Description"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let rows: Vec<Row> = results
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut row = Row::new(vec![t.name.clone(), t.description.clone()]);
            if i == app.search_cursor {
                row = row.style(theme::item_highlight_style());
            }
            row
        })
        .collect();

    let result_title = if results.is_empty() {
        " No results ".to_string()
    } else {
        format!(" {} result(s) ", results.len())
    };

    let table = Table::new(rows, [Constraint::Length(25), Constraint::Min(40)])
        .header(header)
        .block(theme::default_block().title(result_title))
        .column_spacing(2);

    let mut state = TableState::default();
    if !results.is_empty() {
        state.select(Some(app.search_cursor));
    }
    f.render_stateful_widget(table, chunks[2], &mut state);

    // Help bar
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" Navigate ", theme::shortcut_key_style()),
        Span::raw("↑/↓   "),
        Span::styled(" Add to list ", theme::success_text_style()),
        Span::raw("<Enter>   "),
        Span::styled(" Back ", theme::shortcut_action_style(theme::COLOR_WARNING)),
        Span::raw("<Esc>"),
    ]))
    .block(theme::default_block());
    f.render_widget(help, chunks[3]);
}
