use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Tabs, Wrap},
    Frame,
};

use crate::app::{App, BrowseMode, Tab};
use crate::player::PlayerState;

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tabs
            Constraint::Min(10),    // Main content
            Constraint::Length(8),  // Player + Visualizer
            Constraint::Length(3),  // Status bar
        ])
        .split(f.area());

    draw_tabs(f, app, chunks[0]);
    draw_main_content(f, app, chunks[1]);
    draw_player_and_visualizer(f, app, chunks[2]);
    draw_status_bar(f, app, chunks[3]);
    
    // Draw popups on top of everything
    if app.help_popup {
        draw_help_popup(f);
    }
    
    // Draw error popup on top of everything if present
    if app.error_popup.is_some() {
        draw_error_popup(f, app);
    }
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let titles = vec!["Browse (1)", "Favorites (2)", "History (3)"];
    let selected = match app.current_tab {
        Tab::Browse => 0,
        Tab::Favorites => 1,
        Tab::History => 2,
    };

    let tabs = Tabs::new(titles)
        .block(Block::default().borders(Borders::ALL).title("Tabs"))
        .select(selected)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs, area);
}

fn draw_main_content(f: &mut Frame, app: &mut App, area: Rect) {
    match app.current_tab {
        Tab::Browse => draw_browse_tab(f, app, area),
        Tab::Favorites => draw_station_list(f, app, area, "Favorites"),
        Tab::History => draw_station_list(f, app, area, "Recently Played"),
    }
}

fn draw_browse_tab(f: &mut Frame, app: &mut App, area: Rect) {
    if app.browse_list_mode {
        // Show browse list (countries, genres, languages)
        draw_browse_list(f, app, area);
    } else {
        // Show station list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(5)])
            .split(area);

        draw_browse_mode_selector(f, app, chunks[0]);
        draw_station_list(f, app, chunks[1], "Stations");
    }
}

fn draw_browse_mode_selector(f: &mut Frame, app: &App, area: Rect) {
    let modes = vec![
        ("Popular (F1)", BrowseMode::Popular),
        ("Search (/)", BrowseMode::Search),
        ("Country (F2)", BrowseMode::ByCountry),
        ("Genre (F3)", BrowseMode::ByGenre),
        ("Language (F4)", BrowseMode::ByLanguage),
    ];

    let items: Vec<Line> = modes
        .iter()
        .map(|(label, mode)| {
            let style = if *mode == app.browse_mode {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            Line::from(Span::styled(*label, style))
        })
        .collect();

    let paragraph = Paragraph::new(items)
        .block(Block::default().borders(Borders::ALL).title("Browse Mode"))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
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

    f.render_widget(list, area);
}

fn draw_station_list(f: &mut Frame, app: &App, area: Rect, title: &str) {
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

    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!("{} ({} stations)", title, app.stations.len())),
    );

    f.render_widget(list, area);
}

fn draw_player_and_visualizer(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    draw_player(f, app, chunks[0]);
    draw_visualizer(f, app, chunks[1]);
}

fn draw_player(f: &mut Frame, app: &App, area: Rect) {
    let info = &app.player_info;

    let state_text = match info.state {
        PlayerState::Playing => "▶ Playing",
        PlayerState::Paused => "⏸ Paused",
        PlayerState::Stopped => "⏹ Stopped",
        PlayerState::Loading => "⏳ Loading...",
        PlayerState::Error => "❌ Error",
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

fn draw_visualizer(f: &mut Frame, app: &App, area: Rect) {
    let inner = Block::default()
        .borders(Borders::ALL)
        .title("Visualizer")
        .inner(area);

    let block = Block::default().borders(Borders::ALL).title("Visualizer");
    f.render_widget(block, area);

    if inner.height < 2 || inner.width < 2 {
        return;
    }

    let bar_width = (inner.width as usize / app.visualizer_data.len()).max(1);
    let max_height = inner.height.saturating_sub(1) as usize;

    for (i, &value) in app.visualizer_data.iter().enumerate() {
        let bar_height = (value * max_height as f32) as u16;
        let x = inner.x + (i * bar_width) as u16;

        // Use different characters and colors based on height for a cooler effect
        for y in 0..bar_height.min(max_height as u16) {
            let cell_y = inner.y + inner.height - 1 - y;
            if cell_y >= inner.y && cell_y < inner.y + inner.height {
                if let Some(cell) = f.buffer_mut().cell_mut((x, cell_y)) {
                    // Calculate height percentage for color gradients
                    let height_percent = y as f32 / bar_height.max(1) as f32;
                    
                    // Choose character based on position
                    let ch = if y == bar_height - 1 {
                        '▀' // Top of bar
                    } else if height_percent > 0.7 {
                        '█' // Solid block for top portion
                    } else if height_percent > 0.4 {
                        '▓' // Medium shade
                    } else {
                        '▒' // Light shade
                    };
                    
                    // Color gradient from blue (bottom) to cyan to yellow to red (top)
                    let color = if height_percent > 0.85 {
                        Color::Red
                    } else if height_percent > 0.65 {
                        Color::LightRed
                    } else if height_percent > 0.45 {
                        Color::Yellow
                    } else if height_percent > 0.25 {
                        Color::LightCyan
                    } else {
                        Color::Cyan
                    };
                    
                    cell.set_char(ch);
                    cell.set_fg(color);
                }
            }
        }
    }
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect) {
    let mut lines = vec![];

    // Search mode
    if app.search_mode {
        lines.push(Line::from(vec![
            Span::styled("Search: ", Style::default().fg(Color::Cyan)),
            Span::raw(&app.search_query),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::styled(" (Press Enter to search, Esc to cancel)", Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Status message
    if let Some(ref msg) = app.status_message {
        lines.push(Line::from(Span::styled(msg, Style::default().fg(Color::Green))));
    }

    // Main shortcuts line - always show when not in search mode
    if !app.search_mode {
        lines.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(" Nav  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" Play  "),
            Span::styled("Space", Style::default().fg(Color::Yellow)),
            Span::raw(" Pause  "),
            Span::styled("S", Style::default().fg(Color::Yellow)),
            Span::raw(" Stop  "),
            Span::styled("+-", Style::default().fg(Color::Yellow)),
            Span::raw(" Vol  "),
            Span::styled("F", Style::default().fg(Color::Yellow)),
            Span::raw(" Fav  "),
            Span::styled("?", Style::default().fg(Color::Yellow)),
            Span::raw(" Help  "),
            Span::styled("Q", Style::default().fg(Color::Yellow)),
            Span::raw(" Quit"),
        ]));
    }

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Status"))
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
            Span::styled("  Tab         ", Style::default().fg(Color::Yellow)),
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
            Span::styled("  q           ", Style::default().fg(Color::Yellow)),
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
