use ratatui::{
    layout::{Constraint, Direction, Layout},
    text::{Line, Span},
    Frame,
};

use tui_kit::{
    tabs::tab_line,
    toast::render_toasts,
    Theme,
};

use crate::app::{App, Tab};

use super::autovote::draw_autovote_tab;
use super::help_popup::draw_help_popup;
use super::player::draw_player_and_log;
use super::popups::{draw_confirm_delete_popup, draw_error_popup, draw_warning_popup};
use super::station_list::draw_station_list;
use super::status_bar::draw_status_bar;

/// Truncate `s` to at most `col_width` display columns (wide chars count as 2),
/// then pad with spaces on the right so the result is exactly `col_width` columns wide.
pub(super) fn display_col(s: &str, col_width: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
        if used + w > col_width {
            break;
        }
        out.push(ch);
        used += w;
    }
    if used < col_width {
        out.push_str(&" ".repeat(col_width - used));
    }
    out
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let theme = Theme::default();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8), // Player + Status log section (at top)
            Constraint::Min(10),   // Main content (includes tabs in title)
            Constraint::Length(1), // Shortcuts bar (no border)
        ])
        .split(f.area());

    draw_player_and_log(f, app, chunks[0], &theme);
    draw_main_content(f, app, chunks[1], &theme);
    draw_status_bar(f, app, chunks[2], &theme);

    // Draw popups on top of everything
    if app.help_popup {
        draw_help_popup(f, app, &theme);
    }

    if let Some(ref popup) = app.search_popup {
        popup.render(f, f.area(), &theme);
    }

    if app.error_popup.is_some() {
        draw_error_popup(f, app, &theme);
    }

    if app.warning_popup.is_some() {
        draw_warning_popup(f, app, &theme);
    }

    if app.confirm_delete.is_some() {
        draw_confirm_delete_popup(f, app, &theme);
    }

    render_toasts(f, &app.toasts, &theme);
}

fn draw_main_content(f: &mut Frame, app: &mut App, area: ratatui::layout::Rect, theme: &Theme) {
    let has_autovote = app.config.auto_vote_favorites;
    let mut tabs: Vec<(&str, bool)> = vec![
        ("Browse", matches!(app.current_tab, Tab::Browse)),
        ("Favorites", matches!(app.current_tab, Tab::Favorites)),
        ("History", matches!(app.current_tab, Tab::History)),
    ];
    if has_autovote {
        tabs.push(("Autovote", matches!(app.current_tab, Tab::Autovote)));
    }
    let tab_title = tab_line(&tabs, theme);

    let station_focused = !app.has_popup();
    let station_border_style = if station_focused { theme.border_focused } else { theme.border_unfocused };
    let mut title_spans = vec![Span::styled("\u{2500} ", station_border_style)];
    title_spans.extend(tab_title.spans);
    let station_title = Line::from(title_spans);

    match app.current_tab {
        Tab::Autovote => draw_autovote_tab(f, app, area, station_title, theme),
        _ => draw_station_list(f, app, area, station_title, theme),
    }
}
