use crate::app::{App, Screen};
use crate::registry::{Group, InstallStatus, SelectionState};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, ListState, Paragraph},
    Frame,
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
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // ── Build list items with group section headers ──────────────────────────
    //
    // Strategy (Option A): app.cursor indexes into app.components directly.
    // We build a display list that interleaves non-selectable header rows with
    // component rows. A separate vec maps display rows back to component indices.

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
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )]));
            items.push(header);
            row_to_component.push(None);
        }

        let prefix = match c.state {
            SelectionState::Selected => "[x] ",
            SelectionState::Unselected => "[ ] ",
            SelectionState::KeepAsIs => "[-] ",
        };
        let prefix_style = match c.state {
            SelectionState::Selected => Style::default().fg(Color::Green),
            SelectionState::Unselected => Style::default().fg(Color::DarkGray),
            SelectionState::KeepAsIs => Style::default().fg(Color::Yellow),
        };
        let status_text = match &c.status {
            InstallStatus::NotInstalled => " (Not Installed)".to_string(),
            InstallStatus::Installed(v) => format!(" ({})", v),
            _ => String::new(),
        };

        let line = Line::from(vec![
            Span::raw("  "), // indent under header
            Span::styled(prefix, prefix_style),
            Span::styled(&c.name, Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" — {}{}", c.description, status_text)),
        ]);

        // Highlight the row if this component is the current cursor position
        let mut item = ListItem::new(line);
        if comp_idx == app.cursor {
            item = item.style(
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            );
        }
        items.push(item);
        row_to_component.push(Some(comp_idx));
    }

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Select Tools to Install "));

    // Point ratatui's scroll offset at the display row for app.cursor
    let display_row = row_to_component
        .iter()
        .position(|r| *r == Some(app.cursor))
        .unwrap_or(0);
    let mut state = ListState::default();
    state.select(Some(display_row));
    f.render_stateful_widget(list, chunks[1], &mut state);

    // ── Shortcut guide ───────────────────────────────────────────────────────
    let help_text = vec![
        Line::from(vec![
            Span::styled(" Navigate ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("↑/↓  j/k   "),
            Span::styled(" Toggle ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("<Space>   "),
            Span::styled(" Select All ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("a   "),
            Span::styled(" Deselect All ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("n"),
        ]),
        Line::from(vec![
            Span::styled(" Search Registry ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::raw("/   "),
            Span::styled(" Install ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("<Enter>   "),
            Span::styled(" Quit ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("q   "),
            Span::styled(" [-] ", Style::default().fg(Color::Yellow)),
            Span::raw("= keep existing config"),
        ]),
    ];

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL).title(" Shortcuts "));
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
        .block(
            Block::default()
                .title(format!(
                    " Installing: {} ({}/{}) ",
                    label,
                    phase + 1,
                    PHASES as usize
                ))
                .borders(Borders::ALL),
        )
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
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
    let text: String = display_logs.join("\n");
    let p = Paragraph::new(text)
        .block(Block::default().title(" Log Stream ").borders(Borders::ALL));
    f.render_widget(p, chunks[1]);
}

fn draw_report(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(3)])
        .split(f.area());

    let mut lines = vec![
        Line::from(Span::styled(
            "Installation Complete!",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    for c in &app.components {
        let status = match c.state {
            SelectionState::Unselected => "Skipped".to_string(),
            SelectionState::KeepAsIs => "Skipped (Kept as-is)".to_string(),
            SelectionState::Selected => match &c.status {
                InstallStatus::Installed(_) => "Installed / Updated".to_string(),
                _ => "Processed".to_string(),
            },
        };
        let color = if status.starts_with("Failed") {
            Color::Red
        } else if status.starts_with("Skip") {
            Color::DarkGray
        } else {
            Color::Green
        };
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<20}", c.name),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::styled(status, Style::default().fg(color)),
        ]));
    }

    let p = Paragraph::new(lines)
        .block(Block::default().title(" Summary Report ").borders(Borders::ALL));
    f.render_widget(p, chunks[0]);

    let footer = Paragraph::new(" Press 'q' or <Enter> to exit ")
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[1]);
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
        .style(
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        )
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    // Search input
    let input_text = format!(" {} ", app.search_query);
    let input = Paragraph::new(Line::from(vec![
        Span::styled("Search: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(&input_text),
        Span::styled("█", Style::default().fg(Color::Cyan)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(input, chunks[1]);

    // Results list
    let results = &app.search_results;
    let items: Vec<ListItem> = results
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let line = Line::from(vec![
                Span::styled(
                    format!("{:<20}", t.name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" — {}", t.description)),
            ]);
            let mut item = ListItem::new(line);
            if i == app.search_cursor {
                item = item.style(
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD),
                );
            }
            item
        })
        .collect();

    let result_title = if results.is_empty() {
        " No results ".to_string()
    } else {
        format!(" {} result(s) ", results.len())
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(result_title));

    let mut state = ListState::default();
    if !results.is_empty() {
        state.select(Some(app.search_cursor));
    }
    f.render_stateful_widget(list, chunks[2], &mut state);

    // Help bar
    let help = Paragraph::new(Line::from(vec![
        Span::styled(" Navigate ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("↑/↓   "),
        Span::styled(" Add to list ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
        Span::raw("<Enter>   "),
        Span::styled(" Back ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw("<Esc>"),
    ]))
    .block(Block::default().borders(Borders::ALL));
    f.render_widget(help, chunks[3]);
}
