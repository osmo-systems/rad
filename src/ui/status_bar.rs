use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use tui_kit::Theme;

use crate::app::{App, HelpTab};
use rad::PlayerState;

pub(super) fn draw_status_bar(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    // Add horizontal margin to align with bordered widgets
    let margin_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let content_area = margin_chunks[1];

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(20)])
        .split(content_area);

    // Build contextual shortcuts depending on the active UI layer
    let mut pairs: Vec<(&str, &str)> = Vec::new();

    if app.error_popup.is_some() {
        pairs.push(("Esc/Enter", ":Close"));
        pairs.push(("Ctrl+C", ":Quit"));
    } else if app.warning_popup.is_some() {
        pairs.push(("Esc/Enter", ":Close"));
    } else if app.help_popup {
        match app.help_tab {
            HelpTab::Keys => {
                pairs.push(("Tab", ":Settings"));
                pairs.push(("Esc/?", ":Close"));
            }
            HelpTab::Settings => {
                pairs.push(("↑↓", ":Navigate"));
                pairs.push(("←→/Enter", ":Change"));
                pairs.push(("Tab", ":Log"));
                pairs.push(("Esc", ":Close"));
            }
            HelpTab::Log => {
                pairs.push(("↑↓", ":Scroll"));
                pairs.push(("f", ":Filter"));
                pairs.push(("Tab", ":Keys"));
                pairs.push(("Esc", ":Close"));
            }
        }
    } else if app.search_popup.is_some() {
        pairs.push(("Enter", ":Search"));
        pairs.push(("Tab", ":Complete"));
        pairs.push(("↑↓", ":Suggestions"));
        pairs.push(("Esc", ":Cancel"));
    } else {
        // Main screen
        if app.stations.is_empty() {
            pairs.push(("/", ":Search"));
            pairs.push(("F1", ":Popular"));
        } else {
            pairs.push(("↑↓", ":Nav"));
            pairs.push(("Enter", ":Play"));

            match app.player_info.state {
                PlayerState::Playing | PlayerState::Paused => {
                    pairs.push(("Space", ":Pause"));
                    pairs.push(("s", ":Stop"));
                    pairs.push(("r", ":Reload"));
                }
                PlayerState::Loading => {
                    pairs.push(("s", ":Stop"));
                }
                _ => {}
            }

            pairs.push(("+-", ":Vol"));
            pairs.push(("f", ":Fav"));
            pairs.push(("v", ":Vote"));
            pairs.push(("V", ":Autovote"));
            pairs.push(("/", ":Search"));
            pairs.push(("np", ":Page"));
            pairs.push(("Tab", ":Tabs"));
        }
        pairs.push(("?", ":Help"));
        pairs.push(("Ctrl+C", ":Quit"));
    }

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" | ", theme.hint));
        }
        spans.push(Span::styled(*key, theme.shortcut_key));
        spans.push(Span::styled(*desc, theme.hint));
    }

    let shortcuts = Paragraph::new(Line::from(spans)).alignment(Alignment::Left);
    f.render_widget(shortcuts, chunks[0]);

    // Version info on the right
    let version = env!("CARGO_PKG_VERSION");
    let version_line = Line::from(vec![
        Span::styled("rad ", theme.shortcut_key),
        Span::styled(version, theme.hint),
    ]);
    let version_widget = Paragraph::new(version_line).alignment(Alignment::Right);
    f.render_widget(version_widget, chunks[1]);
}
