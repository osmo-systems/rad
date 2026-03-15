use ratatui::{
    layout::Alignment,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
    Frame,
};

use tui_kit::{
    block::popup_block,
    popup::centered_popup,
    tabs::tab_line,
    Theme,
};

use ratatui::layout::Rect;

use crate::app::{App, HelpTab};

pub(super) fn draw_help_popup(f: &mut Frame, app: &App, theme: &Theme) {
    let popup_area = centered_popup(f, 0.7, 90, 26);

    let title = tab_line(
        &[
            ("Keys", app.help_tab == HelpTab::Keys),
            ("Settings", app.help_tab == HelpTab::Settings),
            ("Log", app.help_tab == HelpTab::Log),
        ],
        theme,
    );
    let block = popup_block(title, theme);
    f.render_widget(block, popup_area);

    let content_area = Rect {
        x: popup_area.x + 2,
        y: popup_area.y + 2,
        width: popup_area.width.saturating_sub(4),
        height: popup_area.height.saturating_sub(4),
    };

    match app.help_tab {
        HelpTab::Keys => draw_help_keys_content(f, content_area),
        HelpTab::Settings => draw_help_settings_content(f, app, content_area, theme),
        HelpTab::Log => draw_help_log_content(f, app, content_area),
    }
}

fn draw_help_keys_content(f: &mut Frame, area: Rect) {
    let help_text = vec![
        Line::from(vec![Span::styled(
            "Navigation",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled("  ↑/↓         ", Style::default().fg(Color::Yellow)),
            Span::raw("Navigate station list"),
        ]),
        Line::from(vec![
            Span::styled("  Tab         ", Style::default().fg(Color::Yellow)),
            Span::raw("Cycle tabs (Browse / Favorites / History / Autovote)"),
        ]),
        Line::from(vec![
            Span::styled("  1/2/3/4     ", Style::default().fg(Color::Yellow)),
            Span::raw("Jump to Browse / Favorites / History / Autovote tab"),
        ]),
        Line::from(vec![
            Span::styled("  n / p       ", Style::default().fg(Color::Yellow)),
            Span::raw("Next / previous page (Browse tab only)"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Playback",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled("  Enter       ", Style::default().fg(Color::Yellow)),
            Span::raw("Play selected station"),
        ]),
        Line::from(vec![
            Span::styled("  Space       ", Style::default().fg(Color::Yellow)),
            Span::raw("Pause/Resume playback"),
        ]),
        Line::from(vec![
            Span::styled("  s           ", Style::default().fg(Color::Yellow)),
            Span::raw("Close playback"),
        ]),
        Line::from(vec![
            Span::styled("  r           ", Style::default().fg(Color::Yellow)),
            Span::raw("Reload current station"),
        ]),
        Line::from(vec![
            Span::styled("  + / -       ", Style::default().fg(Color::Yellow)),
            Span::raw("Increase/Decrease volume"),
        ]),
        Line::from(""),
        Line::from(vec![Span::styled(
            "Browse & Search",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )]),
        Line::from(vec![
            Span::styled("  F1          ", Style::default().fg(Color::Yellow)),
            Span::raw("Show popular stations"),
        ]),
        Line::from(vec![
            Span::styled("  /           ", Style::default().fg(Color::Yellow)),
            Span::raw("Search by name"),
        ]),
        Line::from(vec![
            Span::styled("  F2/F3/F4    ", Style::default().fg(Color::Yellow)),
            Span::raw("Browse by Country/Genre/Language"),
        ]),
        Line::from(vec![
            Span::styled("  f           ", Style::default().fg(Color::Yellow)),
            Span::raw("Toggle favorite on selected station"),
        ]),
        Line::from(vec![
            Span::styled("  v           ", Style::default().fg(Color::Yellow)),
            Span::raw("Vote for selected station"),
        ]),
        Line::from(vec![
            Span::styled("  V           ", Style::default().fg(Color::Yellow)),
            Span::raw("Toggle selected station in autovote list"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Ctrl+C      ", Style::default().fg(Color::Yellow)),
            Span::raw("Quit application"),
        ]),
    ];

    f.render_widget(Paragraph::new(help_text).alignment(Alignment::Left), area);
}

fn draw_help_settings_content(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let label_width = 22usize;

    let toast_duration_label = if app.config.toast_duration_secs == 0 {
        "Off".to_string()
    } else {
        format!("{}s", app.config.toast_duration_secs)
    };
    let settings: &[(&str, &str)] = &[
        ("Startup Tab", app.config.startup_tab.label()),
        ("Default Search Order", app.config.default_search_order.label()),
        ("Play at Startup", if app.config.play_at_startup { "On" } else { "Off" }),
        ("Autovote", if app.config.auto_vote_favorites { "On" } else { "Off" }),
        ("Show Logo", if app.config.show_logo { "On" } else { "Off" }),
        ("Toast Duration", &toast_duration_label),
    ];

    let mut lines = vec![Line::from("")];

    for (i, (label, value)) in settings.iter().enumerate() {
        let is_selected = i == app.settings_selected;
        let padding = label_width.saturating_sub(label.len());
        let label_part = format!("  {}{}", label, " ".repeat(padding));
        let value_part = format!("[ {} ]", value);

        let row = Line::from(vec![
            Span::styled(
                label_part,
                if is_selected { theme.tab_active } else { theme.body },
            ),
            Span::styled(
                value_part,
                if is_selected { theme.selection } else { theme.shortcut_key },
            ),
        ]);
        lines.push(row);
        lines.push(Line::from(""));
    }

    f.render_widget(Paragraph::new(lines).alignment(Alignment::Left), area);
}

fn draw_help_log_content(f: &mut Frame, app: &App, area: Rect) {
    // Reserve top line for filter indicator
    let filter_area = Rect { height: 1, ..area };
    let log_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };

    // Draw filter indicator
    let (filter_label, filter_color) = match app.log_level_filter {
        None => ("Filter: All  [f] cycle", Color::DarkGray),
        Some(tui_kit::LogLevel::Error) => ("Filter: Error  [f] cycle", Color::Red),
        Some(tui_kit::LogLevel::Warning) => ("Filter: Warning  [f] cycle", Color::Yellow),
        Some(tui_kit::LogLevel::Info) => ("Filter: Info  [f] cycle", Color::White),
        Some(tui_kit::LogLevel::Debug) => ("Filter: Debug  [f] cycle", Color::DarkGray),
    };
    f.render_widget(
        Paragraph::new(filter_label).style(Style::default().fg(filter_color)),
        filter_area,
    );

    let filtered: Vec<_> = app
        .status_log
        .iter()
        .filter(|e| match app.log_level_filter {
            None => true,
            Some(level) => e.level == level,
        })
        .collect();

    if filtered.is_empty() {
        f.render_widget(
            Paragraph::new("No logs.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            log_area,
        );
        return;
    }

    let count = filtered.len();
    let visible = log_area.height as usize;
    let max_scroll = count.saturating_sub(visible);
    let scroll = app.help_log_scroll.min(max_scroll);
    let end = count.min(scroll + visible);

    let list_items: Vec<ListItem> = filtered[scroll..end]
        .iter()
        .map(|entry| {
            let msg_color = entry.level.color();
            let line = Line::from(vec![
                Span::styled("[", Style::default().fg(Color::Indexed(2))),
                Span::styled(entry.timestamp.as_str(), Style::default().fg(Color::Indexed(6))),
                Span::styled("] ", Style::default().fg(Color::Indexed(2))),
                Span::styled(entry.message.as_str(), Style::default().fg(msg_color)),
            ]);
            ListItem::new(line)
        })
        .collect();

    f.render_widget(List::new(list_items), log_area);
}
