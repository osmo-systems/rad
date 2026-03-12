use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Padding, Paragraph, Wrap},
    Frame,
};

use tui_kit::{
    block::{panel_block, popup_block, widget_title},
    popup::centered_popup,
    tabs::tab_line,
    toast::render_toasts,
    Theme,
};

use crate::app::{App, ConfirmDelete, HelpTab, Tab};
use rad_core::PlayerState;

/// Truncate `s` to at most `max_display_width` display columns (wide chars count as 2),
/// then pad with spaces on the right so the result is exactly `col_width` columns wide.
fn display_col(s: &str, col_width: usize) -> String {
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
    // Pad to col_width display columns
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

    // Draw search popup
    if let Some(ref popup) = app.search_popup {
        popup.render(f, f.area(), &theme);
    }

    // Draw error popup on top of everything if present
    if app.error_popup.is_some() {
        draw_error_popup(f, app, &theme);
    }

    // Draw warning popup on top of everything if present
    if app.warning_popup.is_some() {
        draw_warning_popup(f, app, &theme);
    }

    // Draw confirm delete popup
    if app.confirm_delete.is_some() {
        draw_confirm_delete_popup(f, app, &theme);
    }

    // Draw toast notifications (top-right, non-interactable)
    render_toasts(f, &app.toasts, &theme);
}

fn draw_main_content(f: &mut Frame, app: &mut App, area: Rect, theme: &Theme) {
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

fn draw_radio_art(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let body_style = Style::default().fg(Color::DarkGray);

    let art: Vec<Line> = match app.player_info.state {
        PlayerState::Playing => {
            // 4-phase breathing cycle mapped onto the 8-frame animation clock:
            //   0-1 → small   2-3 → medium   4-5 → large   6-7 → medium
            let phase = match app.animation_frame % 8 {
                0 | 1 => 0,
                2 | 3 => 1,
                4 | 5 => 2,
                _ => 1,
            };
            let (ant_left, ant_right): (&str, &str) = match phase {
                0 => ("(  ", "  )"),
                1 => ("(  (  ", "  )  )"),
                _ => (". (  (  ", "  )  ) ."),
            };
            let signal_color = [Color::DarkGray, Color::Yellow, Color::LightYellow][phase];
            let wave_color = [Color::DarkGray, Color::Gray, Color::White][phase];
            let logo_style = theme.tab_active;
            vec![
                Line::from(vec![
                    Span::styled(ant_left, Style::default().fg(wave_color)),
                    Span::styled("●", Style::default().fg(signal_color).add_modifier(Modifier::BOLD)),
                    Span::styled(ant_right, Style::default().fg(wave_color)),
                ]),
                Line::from(""),
                Line::from(Span::styled("_II_", body_style)),
                Line::from(Span::styled("I||I", body_style)),
                Line::from(Span::styled("██████╗  █████╗ ██████╗", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██╗██   ██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██║██   ██║██████╔╝", logo_style)),
                Line::from(""),
                Line::from(Span::styled("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~", Style::default().fg(wave_color))),
            ]
        }
        PlayerState::Loading => {
            // Tuning/scanning: antenna sweeps asymmetrically left→right like a radar
            // 8-frame sweep cycle; animate_frame wraps at 48 so % 8 still works
            let scan = app.animation_frame % 8;
            let (ant_left, ant_right): (&str, &str) = match scan {
                0 => (". (  (  ", ""),
                1 => ("(  (  ",   ""),
                2 => ("(  ",      ""),
                3 => ("",         ""),
                4 => ("",         "  )"),
                5 => ("",         "  )  )"),
                6 => ("",         "  )  ) ."),
                _ => ("",         "  )"),
            };
            let signal_color = match scan {
                0 | 1 | 5 | 6 => Color::Yellow,
                2 | 4          => Color::Gray,
                _              => Color::DarkGray,
            };
            let wave_color = Style::default().fg(Color::Cyan);
            const SCAN_BARS: &[&str] = &[
                "▸ ·  ·  ·  ·  ·  ·",
                "·  ▸ ·  ·  ·  ·  ·",
                "·  ·  ▸ ·  ·  ·  ·",
                "·  ·  ·  ▸ ·  ·  ·",
                "·  ·  ·  ·  ▸ ·  ·",
                "·  ·  ·  ·  ·  ▸ ·",
                "·  ·  ·  ·  ·  ·  ▸",
                "·  ·  ·  ·  ·  ·  ·",
            ];
            let logo_style = Style::default().fg(Color::Cyan);
            vec![
                Line::from(vec![
                    Span::styled(ant_left, wave_color),
                    Span::styled("●", Style::default().fg(signal_color).add_modifier(Modifier::BOLD)),
                    Span::styled(ant_right, wave_color),
                ]),
                Line::from(""),
                Line::from(Span::styled("_II_", body_style)),
                Line::from(Span::styled("I||I", body_style)),
                Line::from(Span::styled("██████╗  █████╗ ██████╗", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██╗██   ██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██║██   ██║██████╔╝", logo_style)),
                Line::from(""),
                Line::from(Span::styled(SCAN_BARS[scan], Style::default().fg(Color::Cyan))),
            ]
        }
        PlayerState::Paused => {
            // Nightly scene: stars flicker slowly, moon crescent on the right, no wave
            // animation_frame wraps at 48; dividing by 16 gives 3 slow phases (1.6s each)
            let star_phase = (app.animation_frame / 16) % 3;
            let stars_top  = ["*  .     *  .  *  ", ".  *    .  *      ", "*     .     *  .  "][star_phase];
            let stars_moon = [".  *   .   *   ",    "*   .  *   .   ",   ".   *  .  *    "][star_phase];
            let stars_bot  = [".  *  .  *  .  *  ", "*  .  *  .  *  .  ", ".  .  *  .  .  *  "][star_phase];
            let logo_style = Style::default().fg(Color::Cyan);
            let night = Style::default().fg(Color::Blue);
            let moon = Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD);
            vec![
                Line::from(Span::styled(stars_top, night)),
                Line::from(vec![
                    Span::styled(stars_moon, night),
                    Span::styled("☽", moon),
                    Span::styled(" ", night),
                ]),
                Line::from(Span::styled("_II_", body_style)),
                Line::from(Span::styled("I||I", body_style)),
                Line::from(Span::styled("██████╗  █████╗ ██████╗", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██╗██   ██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██║██   ██║██████╔╝", logo_style)),
                Line::from(""),
                Line::from(Span::styled(stars_bot, Style::default().fg(Color::DarkGray))),
            ]
        }
        PlayerState::Error => {
            // Broken antenna: arms splayed, X where signal dot was, flat line at bottom
            let err = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
            let err_dim = Style::default().fg(Color::Red);
            let logo_style = Style::default().fg(Color::Red);
            vec![
                Line::from(vec![
                    Span::styled("\\  ", err_dim),
                    Span::styled("X", err),
                    Span::styled("  /", err_dim),
                ]),
                Line::from(""),
                Line::from(Span::styled("_I\\_", body_style)),
                Line::from(Span::styled("I||I", body_style)),
                Line::from(Span::styled("██████╗  █████╗ ██████╗", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██╗██   ██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██║██   ██║██████╔╝", logo_style)),
                Line::from(""),
                Line::from(Span::styled("- - - - - - - - - - - - -", Style::default().fg(Color::DarkGray))),
            ]
        }
        PlayerState::Stopped => {
            // Quiet/idle: dim logo, no signal
            let dim = Style::default().fg(Color::DarkGray);
            let logo_style = Style::default().fg(Color::DarkGray);
            vec![
                Line::from(Span::styled("(  ·  )", dim)),
                Line::from(""),
                Line::from(Span::styled("_II_", body_style)),
                Line::from(Span::styled("I||I", body_style)),
                Line::from(Span::styled("██████╗  █████╗ ██████╗", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██╗██   ██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██╗██   ██║██   ██║", logo_style)),
                Line::from(Span::styled("██   ██║██   ██║██████╔╝", logo_style)),
                Line::from(""),
                Line::from(Span::styled("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~", dim)),
            ]
        }
    };

    f.render_widget(Paragraph::new(art).alignment(Alignment::Center), area);
}

fn draw_station_list(f: &mut Frame, app: &mut App, area: Rect, title: Line, theme: &Theme) {
    // Keep the query limit in sync with available rows so the list always fills the screen.
    let visible_count = (area.height.saturating_sub(2)) as usize;
    let visible_count = visible_count.max(1);
    app.visible_stations_count = visible_count;
    if visible_count != app.current_query.limit {
        app.current_query.limit = visible_count;
        app.pages_cache.clear();
        // Only trigger a new search when actually browsing — favorites/history are local.
        if app.current_tab == Tab::Browse {
            app.pending_search = true;
        }
    }

    let border_style = if !app.has_popup() { theme.border_focused } else { theme.border_unfocused };

    // Build title with station count
    let mut title_spans = title.spans;
    if !app.stations.is_empty() {
        title_spans.push(Span::raw(" ("));
        title_spans.push(Span::styled(
            format!("{}", app.stations.len()),
            Style::default().fg(Color::Cyan),
        ));
        title_spans.push(Span::raw(" stations)"));
    }
    let full_title = Line::from(title_spans);

    // Build and render the outer block first so we can work with the inner area
    let block = if matches!(app.current_tab, crate::app::Tab::Browse) && app.current_page > 0 {
        let page_info = if app.is_last_page {
            format!(" Page {} ", app.current_page)
        } else {
            format!(" Page {} ... ", app.current_page)
        };
        Block::default()
            .borders(Borders::ALL)
            .title(full_title)
            .title(
                ratatui::widgets::block::Title::from(Span::styled(page_info, border_style))
                    .alignment(Alignment::Right)
                    .position(ratatui::widgets::block::Position::Top),
            )
            .border_style(border_style)
    } else {
        Block::default()
            .borders(Borders::ALL)
            .title(full_title)
            .border_style(border_style)
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Station list items have a natural maximum width based on their format:
    //   [* /  ][▶/●/○] name - country - codec - bitrate
    // We cap the list widget at this width; any remaining space goes to the logo.
    const MAX_LIST_CONTENT_WIDTH: u16 = 80;
    const ART_WIDTH: u16 = 33;
    const ART_HEIGHT: u16 = 11;

    let list_width = inner.width.min(MAX_LIST_CONTENT_WIDTH);
    let remaining = inner.width.saturating_sub(list_width);
    let show_art = app.config.show_logo && remaining >= ART_WIDTH && inner.height >= ART_HEIGHT;

    let list_area = Rect {
        width: list_width,
        ..inner
    };

    if app.stations.is_empty() {
        let text = if app.loading {
            "Loading stations..."
        } else {
            "No stations found."
        };
        f.render_widget(
            Paragraph::new(text)
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: true }),
            list_area,
        );
    } else {
        let list_items: Vec<ListItem> = app
            .stations
            .iter()
            .enumerate()
            .map(|(i, station)| {
                let is_favorite = app.favorites.is_favorite(&station.station_uuid);
                let is_playing_station = matches!(
                    app.player_info.state,
                    PlayerState::Playing | PlayerState::Paused | PlayerState::Loading
                ) && station.url_resolved == app.player_info.station_url;
                let status_marker = if is_playing_station {
                    "▶"
                } else if station.is_online() {
                    "●"
                } else {
                    "○"
                };
                let is_selected = i == app.selected_index;
                let is_voted = app.vote_manager.has_voted_recently(&station.station_uuid);
                let base_style = if is_selected {
                    theme.selection
                } else if is_voted {
                    Style::default().fg(Color::LightGreen)
                } else {
                    Style::default().fg(Color::White)
                };

                let mut spans = vec![];
                if is_favorite {
                    let star_style = if is_selected {
                        base_style
                    } else {
                        Style::default().fg(Color::Yellow)
                    };
                    spans.push(Span::styled("* ", star_style));
                } else {
                    spans.push(Span::styled("  ", base_style));
                }

                // Status marker: colored by online state when not selected
                let marker_style = if is_selected {
                    base_style
                } else {
                    match status_marker {
                        "▶" => Style::default().fg(Color::Green),
                        "●" => Style::default().fg(Color::Green),
                        _ => Style::default().fg(Color::DarkGray),
                    }
                };
                spans.push(Span::styled(status_marker, marker_style));

                // Per-field colors when not selected; uniform base_style when selected
                let (name, country, codec, bitrate) = if is_selected {
                    (base_style, base_style, base_style, base_style)
                } else {
                    (
                        base_style,
                        Style::default().fg(Color::Cyan),
                        Style::default().fg(Color::Indexed(4)),
                        Style::default().fg(Color::Magenta),
                    )
                };

                const NAME_W: usize = 32;
                const COUNTRY_W: usize = 14;
                const CODEC_W: usize = 6;

                let name_col    = display_col(&station.name, NAME_W);
                let country_col = display_col(&station.country, COUNTRY_W);
                let codec_col   = display_col(&station.format_codec(), CODEC_W);

                spans.push(Span::styled(" ", base_style));
                spans.push(Span::styled(name_col, name));
                spans.push(Span::styled("  ", base_style));
                spans.push(Span::styled(country_col, country));
                spans.push(Span::styled("  ", base_style));
                spans.push(Span::styled(codec_col, codec));
                spans.push(Span::styled("  ", base_style));
                spans.push(Span::styled(station.format_bitrate(), bitrate));

                ListItem::new(Line::from(spans))
            })
            .collect();

        let list = List::new(list_items);
        let mut state = ListState::default();
        state.select(Some(app.selected_index));
        f.render_stateful_widget(list, list_area, &mut state);
    }

    // Render art centred horizontally in the remaining space and vertically in the inner area
    if show_art {
        let h_pad = (remaining - ART_WIDTH) / 2;
        let v_pad = inner.height.saturating_sub(ART_HEIGHT) / 2;
        let art_area = Rect {
            x: inner.x + list_width + h_pad,
            y: inner.y + v_pad,
            width: ART_WIDTH,
            height: ART_HEIGHT,
        };
        draw_radio_art(f, app, art_area, theme);
    }
}

fn draw_player_and_log(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    draw_player(f, app, chunks[0], theme);
    draw_status_log(f, app, chunks[1], theme);
}

fn draw_status_log(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let title = widget_title("Log", None, false, theme);
    let block = panel_block(title, false, theme);

    if app.status_log.is_empty() {
        let paragraph = Paragraph::new("No logs yet")
            .block(block)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray));
        f.render_widget(paragraph, area);
        return;
    }

    // Show only the last 6 entries — no scrolling
    let count = app.status_log.len();
    let start = count.saturating_sub(6);
    let list_items: Vec<ListItem> = app.status_log[start..]
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

    f.render_widget(List::new(list_items).block(block), area);
}

fn get_player_icon(state: PlayerState, frame: usize) -> &'static str {
    match state {
        PlayerState::Playing => {
            // Spinning vinyl record animation
            const PLAYING_ICONS: &[&str] = &["◐", "◓", "◑", "◒"];
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

fn draw_player(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let info = &app.player_info;

    // Get animated icon based on state
    let icon = get_player_icon(info.state, app.animation_frame);

    let state_name = match info.state {
        PlayerState::Playing => "Playing",
        PlayerState::Paused => "Paused",
        PlayerState::Stopped => "Stopped",
        PlayerState::Loading => "Loading...",
        PlayerState::Error => "Error",
    };

    let state_color = match info.state {
        PlayerState::Playing => Color::Green,
        PlayerState::Paused => Color::Yellow,
        PlayerState::Stopped => Color::Gray,
        PlayerState::Loading => Color::Cyan, // Changed from Blue for better visibility
        PlayerState::Error => Color::Red,
    };

    // Shorter volume bar (10 chars instead of 20)
    let volume_bar = {
        let filled = ((info.volume * 10.0).round()) as usize;
        let empty = 10 - filled;
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    };

    // Show the currently playing station name prominently
    // Truncate station name if too long (calculate based on widget width)
    let max_station_length = area.width.saturating_sub(4) as usize; // Account for borders
    let station_display = if !info.station_name.is_empty() {
        let name = &info.station_name;
        if name.len() > max_station_length {
            format!("{}...", &name[..max_station_length.saturating_sub(3)])
        } else {
            name.clone()
        }
    } else {
        "No station selected".to_string()
    };

    // Calculate spacing for state and volume to be on the same line
    // We want: "[Icon] State" on left, "Vol: [bar] XX%" on right
    let volume_text = format!("Vol {} {:.0}%", volume_bar, (info.volume * 100.0).round());
    let state_text_len = icon.len() + 1 + state_name.len(); // icon + space + state name
    let volume_text_len = volume_text.len();
    let available_width = area.width.saturating_sub(4) as usize; // Account for borders and padding
    let spacing = if available_width > state_text_len + volume_text_len {
        available_width.saturating_sub(state_text_len + volume_text_len)
    } else {
        2 // Minimum spacing
    };

    let lines = vec![
        Line::from(""), // Empty line for spacing
        Line::from(vec![Span::styled(
            &station_display,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )]),
        Line::from(""), // Empty line for spacing
        Line::from(vec![
            Span::styled(icon, Style::default().fg(state_color)),
            Span::raw(" "),
            Span::styled(
                state_name,
                Style::default()
                    .fg(state_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" ".repeat(spacing)),
            Span::styled("Vol ", Style::default().fg(Color::Cyan)),
            Span::styled(&volume_bar, Style::default().fg(Color::Cyan)),
            Span::raw(format!(" {:.0}%", (info.volume * 100.0).round())),
        ]),
        Line::from(""), // Empty line for spacing
    ];

    let title = widget_title("Now Playing", None, false, theme);
    let block = panel_block(title, false, theme);

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Center);

    f.render_widget(paragraph, area);
}

fn draw_status_bar(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
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

fn draw_error_popup(f: &mut Frame, app: &App, theme: &Theme) {
    if let Some(ref error_msg) = app.error_popup {
        // Calculate popup area (centered, 60% width, auto height based on content)
        let area = f.area();
        let popup_width = (area.width as f32 * 0.6).min(80.0) as u16;

        // Calculate height based on text content
        // Account for: borders (2), error message lines, empty line (1), footer (1)
        let content_width = popup_width.saturating_sub(4) as usize; // -4 for borders and padding

        // Estimate wrapped lines: count characters and divide by content width
        let estimated_lines = (error_msg.len() as f32 / content_width as f32).ceil() as u16;
        let popup_height = (estimated_lines + 2)
            .max(4)
            .min(area.height.saturating_sub(4));

        let popup_area = centered_popup(f, 0.6, 80, popup_height);

        // Create the error message with wrapping
        let error_text = vec![Line::from(Span::raw(error_msg))];

        let paragraph = Paragraph::new(error_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.border_error)
                    .title(" Error ")
                    .title_style(theme.border_error),
            )
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);

        f.render_widget(paragraph, popup_area);
    }
}

fn draw_warning_popup(f: &mut Frame, app: &App, theme: &Theme) {
    if let Some(ref warning_msg) = app.warning_popup {
        let popup_area = centered_popup(f, 0.6, 80, 7);

        let paragraph = Paragraph::new(Line::from(Span::raw(warning_msg.as_str())))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(theme.border_warning)
                    .title(" Warning ")
                    .title_style(theme.border_warning)
                    .padding(Padding::uniform(1)),
            )
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center);

        f.render_widget(paragraph, popup_area);
    }
}

fn draw_autovote_tab(f: &mut Frame, app: &mut App, area: Rect, title: Line, theme: &Theme) {
    let border_style = if !app.has_popup() { theme.border_focused } else { theme.border_unfocused };

    let stations = app.autovote.get_all();
    let mut title_spans = title.spans;
    if !stations.is_empty() {
        title_spans.push(Span::raw(" ("));
        title_spans.push(Span::styled(format!("{}", stations.len()), Style::default().fg(Color::Cyan)));
        title_spans.push(Span::raw(" stations)"));
    }
    let full_title = Line::from(title_spans);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(full_title)
        .border_style(border_style);

    let inner = block.inner(area);
    f.render_widget(block, area);

    if stations.is_empty() {
        f.render_widget(
            Paragraph::new("No autovote stations. Press V on a station to add it.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    const NAME_W: usize = 32;
    const COUNTRY_W: usize = 14;
    const CODEC_W: usize = 6;

    let autovote_selected = app.autovote_selected;
    let list_items: Vec<ListItem> = stations
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let is_selected = i == autovote_selected;
            let base_style = if is_selected { theme.selection } else { Style::default().fg(Color::White) };

            let (name_s, country_s, codec_s, bitrate_s) = if is_selected {
                (base_style, base_style, base_style, base_style)
            } else {
                (
                    base_style,
                    Style::default().fg(Color::Cyan),
                    Style::default().fg(Color::Indexed(4)),
                    Style::default().fg(Color::Magenta),
                )
            };

            let name_col    = display_col(&s.name, NAME_W);
            let country_col = display_col(&s.country, COUNTRY_W);
            let codec_col   = display_col(if s.codec.is_empty() { "—" } else { &s.codec }, CODEC_W);
            let bitrate_str = if s.bitrate > 0 { format!("{} kbps", s.bitrate) } else { "—".to_string() };

            let line = Line::from(vec![
                Span::styled("    ", base_style),
                Span::styled(name_col, name_s),
                Span::styled("  ", base_style),
                Span::styled(country_col, country_s),
                Span::styled("  ", base_style),
                Span::styled(codec_col, codec_s),
                Span::styled("  ", base_style),
                Span::styled(bitrate_str, bitrate_s),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list = List::new(list_items);
    let mut state = ListState::default();
    state.select(Some(autovote_selected));
    f.render_stateful_widget(list, inner, &mut state);
}

fn draw_help_log_content(f: &mut Frame, app: &App, area: Rect) {
    if app.status_log.is_empty() {
        f.render_widget(
            Paragraph::new("No logs yet.")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let count = app.status_log.len();
    let visible = area.height as usize;
    let max_scroll = count.saturating_sub(visible);
    let scroll = app.help_log_scroll.min(max_scroll);
    let end = count.min(scroll + visible);

    let list_items: Vec<ListItem> = app.status_log[scroll..end]
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

    f.render_widget(List::new(list_items), area);
}

fn draw_confirm_delete_popup(f: &mut Frame, app: &App, theme: &Theme) {
    let Some(ref target) = app.confirm_delete else { return };

    let (kind, name) = match target {
        ConfirmDelete::Favorite(_, name) => ("Favorites", name.as_str()),
        ConfirmDelete::Autovote(_, name) => ("Autovote", name.as_str()),
    };

    let popup_area = centered_popup(f, 0.5, 60, 7);

    let text = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("Remove "),
            Span::styled(name, Style::default().fg(Color::Cyan)),
            Span::raw(format!(" from {}?", kind)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" Yes    "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" Cancel"),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border_warning)
                .title(format!(" Remove from {} ", kind))
                .title_style(theme.border_warning),
        )
        .alignment(Alignment::Center);

    f.render_widget(paragraph, popup_area);
}

fn draw_help_popup(f: &mut Frame, app: &App, theme: &Theme) {
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
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
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
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
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
        Line::from(vec![Span::styled(
            "Browse & Search",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
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
        (
            "Default Search Order",
            app.config.default_search_order.label(),
        ),
        (
            "Play at Startup",
            if app.config.play_at_startup {
                "On"
            } else {
                "Off"
            },
        ),
        (
            "Autovote",
            if app.config.auto_vote_favorites {
                "On"
            } else {
                "Off"
            },
        ),
        (
            "Show Logo",
            if app.config.show_logo { "On" } else { "Off" },
        ),
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
                if is_selected {
                    theme.tab_active
                } else {
                    theme.body
                },
            ),
            Span::styled(
                value_part,
                if is_selected {
                    theme.selection
                } else {
                    theme.shortcut_key
                },
            ),
        ]);
        lines.push(row);
        lines.push(Line::from(""));
    }

    f.render_widget(Paragraph::new(lines).alignment(Alignment::Left), area);
}
