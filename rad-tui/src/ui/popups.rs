use ratatui::{
    layout::Alignment,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Padding, Paragraph, Wrap},
    Frame,
};

use tui_kit::{
    popup::centered_popup,
    Theme,
};

use crate::app::{App, ConfirmDelete};

pub(super) fn draw_error_popup(f: &mut Frame, app: &App, theme: &Theme) {
    if let Some(ref error_msg) = app.error_popup {
        let area = f.area();
        let popup_width = (area.width as f32 * 0.6).min(80.0) as u16;

        let content_width = popup_width.saturating_sub(4) as usize;
        let estimated_lines = (error_msg.len() as f32 / content_width as f32).ceil() as u16;
        let popup_height = (estimated_lines + 2)
            .max(4)
            .min(area.height.saturating_sub(4));

        let popup_area = centered_popup(f, 0.6, 80, popup_height);

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

pub(super) fn draw_warning_popup(f: &mut Frame, app: &App, theme: &Theme) {
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

pub(super) fn draw_confirm_delete_popup(f: &mut Frame, app: &App, theme: &Theme) {
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
