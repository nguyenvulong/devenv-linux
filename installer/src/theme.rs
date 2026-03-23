use ratatui::{
    style::{Color, Modifier, Style},
    widgets::{Block, BorderType, Borders},
};

pub const COLOR_SUCCESS: Color = Color::Green;
pub const COLOR_ERROR: Color = Color::Red;
pub const COLOR_WARNING: Color = Color::Yellow;
pub const COLOR_INFO: Color = Color::Cyan;
pub const COLOR_MUTED: Color = Color::DarkGray;
pub const COLOR_HIGHLIGHT: Color = Color::Magenta;

pub fn default_block<'a>() -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
}

pub fn title_style() -> Style {
    Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)
}

pub fn header_style() -> Style {
    Style::default()
        .fg(COLOR_WARNING)
        .add_modifier(Modifier::BOLD)
}

pub fn selection_selected_style() -> Style {
    Style::default().fg(COLOR_SUCCESS)
}

pub fn selection_unselected_style() -> Style {
    Style::default().fg(COLOR_MUTED)
}

pub fn selection_keep_style() -> Style {
    Style::default().fg(COLOR_WARNING)
}

pub fn item_highlight_style() -> Style {
    Style::default()
        .bg(COLOR_MUTED)
        .add_modifier(Modifier::BOLD)
}

pub fn shortcut_key_style() -> Style {
    Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)
}

pub fn shortcut_action_style(color: Color) -> Style {
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

pub fn success_text_style() -> Style {
    Style::default()
        .fg(COLOR_SUCCESS)
        .add_modifier(Modifier::BOLD)
}
