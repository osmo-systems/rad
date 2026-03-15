use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use tui_kit::Theme;

use crate::app::{App, Tab};
use rad::PlayerState;

use super::layout::display_col;
use super::player::draw_radio_art;

pub(super) fn draw_station_list(f: &mut Frame, app: &mut App, area: Rect, title: Line, theme: &Theme) {
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

    let block = if matches!(app.current_tab, Tab::Browse) && app.current_page > 0 {
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

    const MAX_LIST_CONTENT_WIDTH: u16 = 80;
    const ART_WIDTH: u16 = 33;
    const ART_HEIGHT: u16 = 12;

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
                let base_style = if is_selected {
                    theme.selection
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

                let recently_voted = app.vote_manager.has_voted_recently(&station.station_uuid);
                let (name, country, codec, bitrate) = if is_selected {
                    (base_style, base_style, base_style, base_style)
                } else {
                    let name_style = if recently_voted {
                        Style::default().fg(Color::LightBlue)
                    } else {
                        base_style
                    };
                    (
                        name_style,
                        Style::default().fg(Color::Cyan),
                        Style::default().fg(Color::Indexed(4)),
                        Style::default().fg(Color::Magenta),
                    )
                };

                const NAME_W: usize = 40;
                const COUNTRY_W: usize = 18;
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
