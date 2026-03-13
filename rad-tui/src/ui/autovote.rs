use ratatui::{
    layout::{Alignment, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};

use tui_kit::Theme;

use crate::app::App;

use super::layout::display_col;
use super::player::draw_radio_art;

pub(super) fn draw_autovote_tab(f: &mut Frame, app: &mut App, area: Rect, title: Line, theme: &Theme) {
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

    const MAX_LIST_CONTENT_WIDTH: u16 = 80;
    const ART_WIDTH: u16 = 33;
    const ART_HEIGHT: u16 = 12;
    let list_width = inner.width.min(MAX_LIST_CONTENT_WIDTH);
    let remaining = inner.width.saturating_sub(list_width);
    let show_art = app.config.show_logo && remaining >= ART_WIDTH && inner.height >= ART_HEIGHT;
    let list_area = Rect { width: list_width, ..inner };

    const NAME_W: usize = 40;
    const COUNTRY_W: usize = 18;
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
    f.render_stateful_widget(list, list_area, &mut state);

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
