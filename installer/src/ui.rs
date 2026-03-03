use crate::app::{App, Screen};
use crate::registry::{SelectionState, InstallStatus};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Gauge},
    Frame,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    match app.screen {
        Screen::Selection => draw_selection(f, app),
        Screen::Installing => draw_installing(f, app),
        Screen::Report => draw_report(f, app),
    }
}

fn draw_selection(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(4),
        ])
        .split(f.area());

    let title = Paragraph::new(" devenv-linux Interactive Installer ")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(title, chunks[0]);

    let items: Vec<ListItem> = app
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let prefix = match c.state {
                SelectionState::Selected => "[x] ",
                SelectionState::Unselected => "[ ] ",
                SelectionState::KeepAsIs => "[-] ", // Only for configs
            };

            let prefix_style = match c.state {
                SelectionState::Selected => Style::default().fg(Color::Green),
                SelectionState::Unselected => Style::default().fg(Color::DarkGray),
                SelectionState::KeepAsIs => Style::default().fg(Color::Yellow),
            };

            let status_text = match &c.status {
                InstallStatus::NotInstalled => " (Not Installed)".to_string(),
                InstallStatus::Installed(v) => format!(" (Installed: {})", v),
                _ => "".to_string()
            };

            let line = Line::from(vec![
                Span::styled(prefix, prefix_style),
                Span::styled(&c.name, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(format!(" - {}{}", c.description, status_text)),
            ]);

            let mut item = ListItem::new(line);
            if i == app.cursor {
                item = item.style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));
            }
            item
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Select Tools to Install "));
    
    // We maintain a pseudo list state so ratatui can offset if list goes beyond screen
    let mut state = ListState::default();
    state.select(Some(app.cursor));
    f.render_stateful_widget(list, chunks[1], &mut state);

    let help_text = " Navigate: ↑/↓ | Toggle: <Space> | Toggle All: a/n | Confirm & Install: <Enter> | Quit: q\n KeepAsIs '[-]' skips configs. Checked '[x]' installs/updates.";
    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::Gray))
        .block(Block::default().borders(Borders::ALL).title(" Help "));
    f.render_widget(help, chunks[2]);
}

fn draw_installing(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
        ])
        .split(f.area());

    // 3 phases: system(0), mise(1), config(2)
    let phase = *app.install_index.lock().unwrap();
    const PHASES: f64 = 3.0;
    let phase_labels = ["System Packages", "Mise Tools", "Configurations"];
    let label = phase_labels.get(phase).copied().unwrap_or("Finishing…");
    let progress = ((phase as f64) / PHASES).clamp(0.0, 1.0);
    let gauge = Gauge::default()
        .block(Block::default()
            .title(format!(" Installing: {} ({}/{}) ", label, phase + 1, PHASES as usize))
            .borders(Borders::ALL))
        .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray))
        .ratio(progress);
    f.render_widget(gauge, chunks[0]);

    let logs = app.logs.lock().unwrap();
    // Show last N lines
    let display_logs = logs.iter().rev().take(f.area().height as usize - 5).rev().cloned().collect::<Vec<String>>();
    let text: String = display_logs.join("\n");
    
    let p = Paragraph::new(text)
        .block(Block::default().title(" Log Stream ").borders(Borders::ALL));
    f.render_widget(p, chunks[1]);
}

fn draw_report(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(f.area());

    let mut lines = vec![
        Line::from(Span::styled("Installation Complete!", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD))),
        Line::from(""),
    ];

    for c in &app.components {
        let status = match c.state {
            SelectionState::Unselected => "Skipped".to_string(),
            SelectionState::KeepAsIs => "Skipped (Kept as is)".to_string(),
            SelectionState::Selected => {
                match &c.status {
                    InstallStatus::Installed(_) => "Installed / Updated".to_string(),
                    _ => "Processed".to_string()
                }
            }
        };
        
        let color = if status.starts_with("Failed") { Color::Red } else if status.starts_with("Skip") { Color::DarkGray } else { Color::Green };
        lines.push(Line::from(vec![
            Span::styled(format!("{:<20}", c.name), Style::default().add_modifier(Modifier::BOLD)),
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
