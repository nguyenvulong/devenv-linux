use crate::app::{App, Screen};
use crate::registry::{Group, InstallStatus, SelectionState};
use crate::theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Gauge, List, ListItem, ListState, Paragraph, Row, Table, TableState},
};

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Selection => draw_selection(f, app),
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

        let prefix = match c.state {
            SelectionState::Selected => "[✓] ",
            SelectionState::Unselected => "[ ] ",
            SelectionState::KeepAsIs => "[~] ",
        };
        let prefix_style = match c.state {
            SelectionState::Selected => theme::selection_selected_style(),
            SelectionState::Unselected => theme::selection_unselected_style(),
            SelectionState::KeepAsIs => theme::selection_keep_style(),
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

        let status_text = match &c.status {
            InstallStatus::NotInstalled => "Not Installed".to_string(),
            InstallStatus::Installed(v) => format!("Installed ({})", v),
            _ => "Unknown".to_string(),
        };
        let status_color = match &c.status {
            InstallStatus::NotInstalled => theme::COLOR_MUTED,
            InstallStatus::Installed(_) => theme::COLOR_SUCCESS,
            _ => theme::COLOR_WARNING,
        };
        details_text.push(Line::from(vec![
            Span::styled("Status: ", theme::shortcut_key_style()),
            Span::styled(status_text, Style::default().fg(status_color)),
        ]));

        let details = Paragraph::new(details_text).block(theme::default_block().title(" Details "));
        f.render_widget(details, main_chunks[1]);
    }

    // ── Shortcut guide ───────────────────────────────────────────────────────
    let help_text = vec![
        Line::from(vec![
            Span::styled(" Navigate ", theme::shortcut_key_style()),
            Span::raw("↑/↓  j/k   "),
            Span::styled(" Toggle ", theme::shortcut_key_style()),
            Span::raw("<Space>   "),
            Span::styled(" Select All ", theme::shortcut_key_style()),
            Span::raw("a   "),
            Span::styled(" Deselect All ", theme::shortcut_key_style()),
            Span::raw("n"),
        ]),
        Line::from(vec![
            Span::styled(
                " Search Registry ",
                theme::shortcut_action_style(theme::COLOR_HIGHLIGHT),
            ),
            Span::raw("/   "),
            Span::styled(" Install ", theme::success_text_style()),
            Span::raw("<Enter>   "),
            Span::styled(" Quit ", theme::shortcut_action_style(theme::COLOR_ERROR)),
            Span::raw("q   "),
            Span::styled(" [~] ", theme::selection_keep_style()),
            Span::raw("= keep existing config"),
        ]),
    ];

    let help = Paragraph::new(help_text).block(theme::default_block().title(" Shortcuts "));
    f.render_widget(help, chunks[2]);
}

fn draw_installing(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(10)])
        .split(f.area());

    let phase = *app.install_index.lock().unwrap();
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

    let logs = app.logs.lock().unwrap();
    let display_logs = logs
        .iter()
        .rev()
        .take(f.area().height as usize - 5)
        .rev()
        .cloned()
        .collect::<Vec<String>>();

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
        " Installation Complete! ",
        theme::success_text_style(),
    ))
    .block(theme::default_block());
    f.render_widget(header_text, chunks[0]);

    let mut installed_count = 0;
    let mut skipped_count = 0;
    let mut rows: Vec<Row> = Vec::new();

    for c in &app.components {
        let (action, status_msg, color) = match c.state {
            SelectionState::Unselected => {
                skipped_count += 1;
                ("Skip", "Not Selected", theme::COLOR_MUTED)
            }
            SelectionState::KeepAsIs => {
                skipped_count += 1;
                ("Skip", "Kept as-is", theme::COLOR_MUTED)
            }
            SelectionState::Selected => {
                installed_count += 1;
                match &c.status {
                    InstallStatus::Installed(_) => {
                        ("Install", "Installed / Updated", theme::COLOR_SUCCESS)
                    }
                    _ => ("Process", "Processed", theme::COLOR_SUCCESS),
                }
            }
        };

        rows.push(Row::new(vec![
            Span::styled(
                c.name.clone(),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(action.to_string(), Style::default().fg(color)),
            Span::styled(status_msg.to_string(), Style::default().fg(color)),
        ]));
    }

    let summary_title = format!(
        " Summary Report [ {} Processed | {} Skipped ] ",
        installed_count, skipped_count
    );

    let table_header = Row::new(vec!["Tool Name", "Action", "Status"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let table = Table::new(
        rows,
        [
            Constraint::Length(25),
            Constraint::Length(15),
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
