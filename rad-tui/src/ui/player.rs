use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
    Frame,
};

use tui_kit::{
    block::{panel_block, widget_title},
    Theme,
};

use crate::app::App;
use rad_core::PlayerState;

pub(super) fn draw_player_and_log(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
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
        PlayerState::Loading => Color::Cyan,
        PlayerState::Error => Color::Red,
    };

    let volume_bar = {
        let filled = ((info.volume * 10.0).round()) as usize;
        let empty = 10 - filled;
        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    };

    let max_station_length = area.width.saturating_sub(4) as usize;
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

    let volume_text = format!("Vol {} {:.0}%", volume_bar, (info.volume * 100.0).round());
    let state_text_len = icon.len() + 1 + state_name.len();
    let volume_text_len = volume_text.len();
    let available_width = area.width.saturating_sub(4) as usize;
    let spacing = if available_width > state_text_len + volume_text_len {
        available_width.saturating_sub(state_text_len + volume_text_len)
    } else {
        2
    };

    let lines = vec![
        Line::from(""),
        Line::from(vec![Span::styled(
            &station_display,
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        )]),
        Line::from(""),
        Line::from(vec![
            Span::styled(icon, Style::default().fg(state_color)),
            Span::raw(" "),
            Span::styled(state_name, Style::default().fg(state_color).add_modifier(Modifier::BOLD)),
            Span::raw(" ".repeat(spacing)),
            Span::styled("Vol ", Style::default().fg(Color::Cyan)),
            Span::styled(&volume_bar, Style::default().fg(Color::Cyan)),
            Span::raw(format!(" {:.0}%", (info.volume * 100.0).round())),
        ]),
        Line::from(""),
    ];

    let title = widget_title("Now Playing", None, false, theme);
    let block = panel_block(title, false, theme);

    let paragraph = Paragraph::new(lines).block(block).alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

pub(super) fn draw_radio_art(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
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
                Line::from(Span::styled("██████╗  █████╗ ██████╗ ", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██╗██╔══██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██║  ██║", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██║██║  ██║", logo_style)),
                Line::from(Span::styled("██║  ██║██║  ██║██████╔╝", logo_style)),
                Line::from(Span::styled("╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝ ", logo_style)),
                Line::from(""),
                Line::from(Span::styled("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~", Style::default().fg(wave_color))),
            ]
        }
        PlayerState::Loading => {
            // Tuning/scanning: antenna sweeps asymmetrically left→right like a radar
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
                Line::from(Span::styled("██████╗  █████╗ ██████╗ ", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██╗██╔══██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██║  ██║", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██║██║  ██║", logo_style)),
                Line::from(Span::styled("██║  ██║██║  ██║██████╔╝", logo_style)),
                Line::from(Span::styled("╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝ ", logo_style)),
                Line::from(""),
                Line::from(Span::styled(SCAN_BARS[scan], Style::default().fg(Color::Cyan))),
            ]
        }
        PlayerState::Paused => {
            // Nightly scene: stars flicker slowly, moon crescent on the right, no wave
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
                Line::from(Span::styled("██████╗  █████╗ ██████╗ ", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██╗██╔══██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██║  ██║", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██║██║  ██║", logo_style)),
                Line::from(Span::styled("██║  ██║██║  ██║██████╔╝", logo_style)),
                Line::from(Span::styled("╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝ ", logo_style)),
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
                Line::from(Span::styled("██████╗  █████╗ ██████╗ ", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██╗██╔══██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██║  ██║", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██║██║  ██║", logo_style)),
                Line::from(Span::styled("██║  ██║██║  ██║██████╔╝", logo_style)),
                Line::from(Span::styled("╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝ ", logo_style)),
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
                Line::from(Span::styled("██████╗  █████╗ ██████╗ ", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██╗██╔══██╗", logo_style)),
                Line::from(Span::styled("██████╔╝███████║██║  ██║", logo_style)),
                Line::from(Span::styled("██╔══██╗██╔══██║██║  ██║", logo_style)),
                Line::from(Span::styled("██║  ██║██║  ██║██████╔╝", logo_style)),
                Line::from(Span::styled("╚═╝  ╚═╝╚═╝  ╚═╝╚═════╝", logo_style)),
                Line::from(""),
                Line::from(Span::styled("~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~", dim)),
            ]
        }
    };

    f.render_widget(Paragraph::new(art).alignment(Alignment::Center), area);
}
