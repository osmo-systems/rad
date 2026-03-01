use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, BrowseMode, Tab};
use crate::player::PlayerState;

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(10),    // Main content (includes tabs in title)
            Constraint::Length(8),  // Player + Status log section
            Constraint::Length(3),  // Shortcuts bar
        ])
        .split(f.area());

    draw_main_content(f, app, chunks[0]);
    draw_player_and_log(f, app, chunks[1]);
    draw_status_bar(f, app, chunks[2]);
    
    // Draw popups on top of everything
    if app.help_popup {
        draw_help_popup(f);
    }
    
    // Draw error popup on top of everything if present
    if app.error_popup.is_some() {
        draw_error_popup(f, app);
    }
}

fn draw_main_content(f: &mut Frame, app: &mut App, area: Rect) {
    // Build tab titles with color highlighting
    let mode_suffix = if matches!(app.current_tab, Tab::Browse) {
        let mode_name = match app.browse_mode {
            BrowseMode::Popular => "Popular",
            BrowseMode::Search => "Search",
            BrowseMode::ByCountry => "By Country",
            BrowseMode::ByGenre => "By Genre",
            BrowseMode::ByLanguage => "By Language",
        };
        format!(": {}", mode_name)
    } else {
        String::new()
    };
    
    let tab_title = Line::from(vec![
        Span::styled(
            format!("Browse(1){}", mode_suffix),
            if matches!(app.current_tab, Tab::Browse) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            }
        ),
        Span::raw("  "),
        Span::styled(
            "Favorites(2)",
            if matches!(app.current_tab, Tab::Favorites) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            }
        ),
        Span::raw("  "),
        Span::styled(
            "History(3)",
            if matches!(app.current_tab, Tab::History) {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            }
        ),
    ]);
    
    match app.current_tab {
        Tab::Browse => draw_browse_tab(f, app, area, tab_title),
        Tab::Favorites => draw_station_list(f, app, area, tab_title),
        Tab::History => draw_station_list(f, app, area, tab_title),
    }
}

fn draw_browse_tab(f: &mut Frame, app: &mut App, area: Rect, title: Line) {
    if app.browse_list_mode {
        // Show browse list (countries, genres, languages)
        draw_browse_list(f, app, area);
    } else {
        // Show station list directly (browse mode shown in tab title)
        draw_station_list(f, app, area, title);
    }
}

fn draw_browse_list(f: &mut Frame, app: &App, area: Rect) {
    let (title, items) = match app.browse_mode {
        BrowseMode::ByCountry => ("Countries", &app.countries),
        BrowseMode::ByGenre => ("Genres", &app.genres),
        BrowseMode::ByLanguage => ("Languages", &app.languages),
        _ => ("", &vec![]),
    };

    let list_items: Vec<ListItem> = items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let style = if i == app.browse_list_index {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(item.as_str()).style(style)
        })
        .collect();

    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{} (Press Enter to select, Esc to go back)", title)),
        );

    let mut state = ListState::default();
    state.select(Some(app.browse_list_index));
    
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_station_list(f: &mut Frame, app: &App, area: Rect, title: Line) {
    if app.stations.is_empty() {
        let text = if app.loading {
            "Loading stations..."
        } else {
            "No stations loaded.\n\nPress F1 to load Popular Stations\nPress / to Search by name\nPress 2 for Countries, 3 for Genres, 4 for Languages"
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

    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(full_title),
    );

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

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let line = if app.search_mode {
        // Search mode display
        Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.search_query),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::styled(" | Enter:Search  Esc:Cancel", Style::default().fg(Color::DarkGray)),
        ])
    } else {
        // Main shortcuts - compact single line
        Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(":Nav  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(":Play  "),
            Span::styled("Space", Style::default().fg(Color::Yellow)),
            Span::raw(":Pause  "),
            Span::styled("S", Style::default().fg(Color::Yellow)),
            Span::raw(":Stop  "),
            Span::styled("+-", Style::default().fg(Color::Yellow)),
            Span::raw(":Vol  "),
            Span::styled("F", Style::default().fg(Color::Yellow)),
            Span::raw(":Fav  "),
            Span::styled("[]", Style::default().fg(Color::Yellow)),
            Span::raw(":Switch tab  "),
            Span::styled("?", Style::default().fg(Color::Yellow)),
            Span::raw(":Help  "),
            Span::styled("Ctrl+C", Style::default().fg(Color::Yellow)),
            Span::raw(":Quit"),
        ])
    };

    let paragraph = Paragraph::new(line)
        .block(Block::default().borders(Borders::ALL).title("Shortcuts"))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
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
            Line::from(Span::styled("Press Esc/Enter to close, Q to quit app", Style::default().fg(Color::Yellow))),
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
