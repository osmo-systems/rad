use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Tab};
use crate::player::PlayerState;

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Player + Status log section (at top)
            Constraint::Min(10),    // Main content (includes tabs in title)
            Constraint::Length(1),  // Shortcuts bar (no border)
        ])
        .split(f.area());

    draw_player_and_log(f, app, chunks[0]);
    draw_main_content(f, app, chunks[1]);
    draw_status_bar(f, app, chunks[2]);
    
    // Draw popups on top of everything
    if app.help_popup {
        draw_help_popup(f);
    }
    
    // Draw search popup
    if let Some(ref popup) = app.search_popup {
        popup.render(f, f.area());
    }
    
    // Draw error popup on top of everything if present
    if app.error_popup.is_some() {
        draw_error_popup(f, app);
    }
    
    // Draw warning popup on top of everything if present
    if app.warning_popup.is_some() {
        draw_warning_popup(f, app);
    }
}

fn draw_main_content(f: &mut Frame, app: &mut App, area: Rect) {
    // Build tab titles
    let tab_title = Line::from(vec![
        Span::styled(
            "Browse",
            if matches!(app.current_tab, Tab::Browse) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            }
        ),
        Span::raw("  "),
        Span::styled(
            "Favorites",
            if matches!(app.current_tab, Tab::Favorites) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            }
        ),
        Span::raw("  "),
        Span::styled(
            "History",
            if matches!(app.current_tab, Tab::History) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            }
        ),
    ]);
    
    // All tabs just show station lists now
    draw_station_list(f, app, area, tab_title);
}

fn draw_station_list(f: &mut Frame, app: &mut App, area: Rect, title: Line) {
    // Calculate visible stations count (area height minus borders and padding)
    // Each station takes 1 line, borders take 2 lines
    let visible_count = (area.height.saturating_sub(2)) as usize;
    app.visible_stations_count = visible_count.max(1);
    
    if app.stations.is_empty() {
        let text = if app.loading {
            "Loading stations..."
        } else {
            "No stations loaded.\n\nPress / to search for stations\nPress F1 to load popular stations"
        };
        
        let paragraph = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title(title))
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });
        
        f.render_widget(paragraph, area);
        return;
    }

    let list_items: Vec<ListItem> = app
        .stations
        .iter()
        .enumerate()
        .map(|(i, station)| {
            let is_favorite = app.favorites.is_favorite(&station.station_uuid);
            let fav_marker = if is_favorite { "♥ " } else { "  " };
            let status_marker = if station.is_online() { "●" } else { "○" };
            
            let content = format!(
                "{}{} {} - {} - {} - {}",
                fav_marker,
                status_marker,
                station.name,
                station.country,
                station.format_codec(),
                station.format_bitrate()
            );

            let style = if i == app.selected_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(content).style(style)
        })
        .collect();

    // Combine title line with station count in a single Line
    let mut title_spans = title.spans;
    title_spans.push(Span::raw(" ("));
    title_spans.push(Span::styled(format!("{}", app.stations.len()), Style::default().fg(Color::Cyan)));
    title_spans.push(Span::raw(" stations)"));
    let full_title = Line::from(title_spans);

    // Create pagination info for the right side if we have pagination
    let block = if app.current_page > 0 {
        let page_info = if app.is_last_page {
            format!("Page {}", app.current_page)
        } else {
            format!("Page {} →", app.current_page)
        };
        Block::default()
            .borders(Borders::ALL)
            .title(full_title)
            .title(
                ratatui::widgets::block::Title::from(
                    Span::styled(page_info, Style::default().fg(Color::Yellow))
                )
                .alignment(Alignment::Right)
                .position(ratatui::widgets::block::Position::Top)
            )
    } else {
        Block::default()
            .borders(Borders::ALL)
            .title(full_title)
    };

    let list = List::new(list_items).block(block);

    let mut state = ListState::default();
    state.select(Some(app.selected_index));
    
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_player_and_log(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_player(f, app, chunks[0]);
    draw_status_log(f, app, chunks[1]);
}

fn draw_status_log(f: &mut Frame, app: &App, area: Rect) {
    if app.status_log.is_empty() {
        let paragraph = Paragraph::new("No status messages yet")
            .block(Block::default().borders(Borders::ALL).title("Status Log"))
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, area);
        return;
    }

    // Create list items from status log
    let list_items: Vec<ListItem> = app
        .status_log
        .iter()
        .map(|msg| ListItem::new(msg.as_str()).style(Style::default().fg(Color::White)))
        .collect();

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title("Status Log"));

    let mut state = ListState::default();
    state.select(Some(app.status_log_scroll));
    
    f.render_stateful_widget(list, area, &mut state);
}

fn get_player_icon(state: PlayerState, frame: usize) -> &'static str {
    match state {
        PlayerState::Playing => {
            const PLAYING_ICONS: &[&str] = &["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
            PLAYING_ICONS[frame % PLAYING_ICONS.len()]
        }
        PlayerState::Loading => {
            const LOADING_ICONS: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
            LOADING_ICONS[frame % LOADING_ICONS.len()]
        }
        PlayerState::Paused => "⏸",
        PlayerState::Stopped => "⏹",
        PlayerState::Error => "❌",
    }
}

fn draw_player(f: &mut Frame, app: &App, area: Rect) {
    let info = &app.player_info;
    
    // Get animated icon based on state
    let icon = get_player_icon(info.state, app.animation_frame);

    let state_text = match info.state {
        PlayerState::Playing => format!("{} Playing", icon),
        PlayerState::Paused => format!("{} Paused", icon),
        PlayerState::Stopped => format!("{} Stopped", icon),
        PlayerState::Loading => format!("{} Loading...", icon),
        PlayerState::Error => format!("{} Error", icon),
    };

    let state_color = match info.state {
        PlayerState::Playing => Color::Green,
        PlayerState::Paused => Color::Yellow,
        PlayerState::Stopped => Color::Gray,
        PlayerState::Loading => Color::Blue,
        PlayerState::Error => Color::Red,
    };

    let volume_bar = {
        let filled = (info.volume * 20.0) as usize;
        let empty = 20 - filled;
        format!("{}{}",  "█".repeat(filled), "░".repeat(empty))
    };

    // Show the currently playing station name prominently
    let station_display = if !info.station_name.is_empty() {
        info.station_name.clone()
    } else {
        "No station selected".to_string()
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(state_text, Style::default().fg(state_color).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("♫ ", Style::default().fg(Color::Magenta)),
            Span::styled(&station_display, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Volume:  ", Style::default().fg(Color::Cyan)),
            Span::raw(format!("{} {}%", volume_bar, (info.volume * 100.0) as u8)),
        ]),
    ];

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Player"))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, _app: &App, area: Rect) {
    // Add horizontal margin to align with bordered widgets
    let margin_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(1),  // Left margin
            Constraint::Min(0),     // Content
            Constraint::Length(1),  // Right margin
        ])
        .split(area);
    
    let content_area = margin_chunks[1];
    
    // Split content area into left (shortcuts) and right (version)
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),      // Shortcuts (takes remaining space)
            Constraint::Length(20),  // Version info
        ])
        .split(content_area);
    
    // Main shortcuts - compact single line with light blue
    let text_style = Style::default().fg(Color::Cyan);
    let key_style = Style::default().fg(Color::LightCyan);
    
    let shortcuts_line = Line::from(vec![
        Span::styled("↑↓", key_style),
        Span::styled(":Nav", text_style),
        Span::styled(" | ", text_style),
        Span::styled("Enter", key_style),
        Span::styled(":Select Station", text_style),
        Span::styled(" | ", text_style),
        Span::styled("Space", key_style),
        Span::styled(":Pause", text_style),
        Span::styled(" | ", text_style),
        Span::styled("S", key_style),
        Span::styled(":Stop", text_style),
        Span::styled(" | ", text_style),
        Span::styled("+-", key_style),
        Span::styled(":Vol", text_style),
        Span::styled(" | ", text_style),
        Span::styled("F", key_style),
        Span::styled(":Fav", text_style),
        Span::styled(" | ", text_style),
        Span::styled("/", key_style),
        Span::styled(":Search", text_style),
        Span::styled(" | ", text_style),
        Span::styled("[]", key_style),
        Span::styled(":Page", text_style),
        Span::styled(" | ", text_style),
        Span::styled("Tab", key_style),
        Span::styled(":Switch Panel", text_style),
        Span::styled(" | ", text_style),
        Span::styled("?", key_style),
        Span::styled(":Help", text_style),
        Span::styled(" | ", text_style),
        Span::styled("Ctrl+C", key_style),
        Span::styled(":Quit", text_style),
    ]);

    let shortcuts = Paragraph::new(shortcuts_line)
        .alignment(Alignment::Left);

    f.render_widget(shortcuts, chunks[0]);
    
    // Version info on the right
    let version = env!("CARGO_PKG_VERSION");
    let version_line = Line::from(vec![
        Span::styled("lazyradio ", text_style),
        Span::styled(version, key_style),
    ]);
    
    let version_widget = Paragraph::new(version_line)
        .alignment(Alignment::Right);
    
    f.render_widget(version_widget, chunks[1]);
}

fn draw_error_popup(f: &mut Frame, app: &App) {
    if let Some(ref error_msg) = app.error_popup {
        // Calculate popup area (centered, 60% width, auto height)
        let area = f.area();
        let popup_width = (area.width as f32 * 0.6).min(80.0) as u16;
        let popup_height = 10u16; // Fixed height for error popup
        
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;
        
        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };
        
        // Clear the background
        f.render_widget(Clear, popup_area);
        
        // Create the error message with wrapping
        let error_text = vec![
            Line::from(Span::styled("Error", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(Span::raw(error_msg)),
            Line::from(""),
            Line::from(Span::styled("Press Esc/Enter to close, Ctrl+C to quit app", Style::default().fg(Color::Yellow))),
        ];
        
        let paragraph = Paragraph::new(error_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Red))
                    .title(" Error ")
                    .title_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            )
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);
        
        f.render_widget(paragraph, popup_area);
    }
}

fn draw_warning_popup(f: &mut Frame, app: &App) {
    if let Some(ref warning_msg) = app.warning_popup {
        // Calculate popup area (centered, 60% width, auto height)
        let area = f.area();
        let popup_width = (area.width as f32 * 0.6).min(80.0) as u16;
        let popup_height = 10u16; // Fixed height for warning popup
        
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;
        
        let popup_area = Rect {
            x: popup_x,
            y: popup_y,
            width: popup_width,
            height: popup_height,
        };
        
        // Clear the background
        f.render_widget(Clear, popup_area);
        
        // Create the warning message with wrapping
        let warning_text = vec![
            Line::from(Span::styled("Warning", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from(Span::raw(warning_msg)),
            Line::from(""),
            Line::from(Span::styled("Press Esc/Enter to close", Style::default().fg(Color::Gray))),
        ];
        
        let paragraph = Paragraph::new(warning_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(" Warning ")
                    .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
            )
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);
        
        f.render_widget(paragraph, popup_area);
    }
}

fn draw_help_popup(f: &mut Frame) {
    // Calculate popup area (centered, 70% width, auto height)
    let area = f.area();
    let popup_width = (area.width as f32 * 0.7).min(90.0) as u16;
    let popup_height = 24u16;
    
    let popup_x = (area.width.saturating_sub(popup_width)) / 2;
    let popup_y = (area.height.saturating_sub(popup_height)) / 2;
    
    let popup_area = Rect {
        x: popup_x,
        y: popup_y,
        width: popup_width,
        height: popup_height,
    };
    
    // Clear the background
    f.render_widget(Clear, popup_area);
    
    // Create the help text
    let help_text = vec![
        Line::from(Span::styled("Keyboard Shortcuts", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::styled("Navigation", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  ↑/↓ or j/k  ", Style::default().fg(Color::Yellow)),
            Span::raw("Navigate station list"),
        ]),
        Line::from(vec![
            Span::styled("  Tab / [ ]   ", Style::default().fg(Color::Yellow)),
            Span::raw("Switch between tabs (Browse/Favorites/History)"),
        ]),
        Line::from(vec![
            Span::styled("  1/2/3       ", Style::default().fg(Color::Yellow)),
            Span::raw("Jump to Browse/Favorites/History tab"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Playback", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
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
            Span::raw("Stop playback"),
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
        Line::from(vec![
            Span::styled("Browse & Search", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        ]),
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
        Line::from(""),
        Line::from(vec![
            Span::styled("  Ctrl+C      ", Style::default().fg(Color::Yellow)),
            Span::raw("Quit application"),
        ]),
        Line::from(""),
        Line::from(Span::styled("Press Esc or ? to close this help", Style::default().fg(Color::DarkGray))),
    ];
    
    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Help - LazyRadio ")
                .title_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))
        )
        .alignment(Alignment::Left);
    
    f.render_widget(paragraph, popup_area);
}
