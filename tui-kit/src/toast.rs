use std::time::Instant;

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

use crate::Theme;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToastLevel {
    Success,
    Warning,
    Error,
}

pub struct Toast {
    pub message: String,
    pub level: ToastLevel,
    created_at: Instant,
    pub duration_ms: u64,
}

impl Toast {
    pub fn new(message: String, level: ToastLevel, duration_ms: u64) -> Self {
        Self {
            message,
            level,
            created_at: Instant::now(),
            duration_ms,
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_millis() as u64 >= self.duration_ms
    }

    /// True when in the last third of life — used to apply the ANSI-8 fade color.
    /// Capped so that even a 1 s toast shows its full color for the first 2/3 of its life.
    fn is_fading(&self) -> bool {
        let elapsed_ms = self.created_at.elapsed().as_millis() as u64;
        let fade_start_ms = self.duration_ms * 2 / 3;
        elapsed_ms >= fade_start_ms
    }

    /// Number of content lines needed to display the message at `content_width`, capped at 3.
    fn content_lines(&self, content_width: usize) -> u16 {
        if content_width == 0 {
            return 1;
        }
        let chars = self.message.chars().count();
        let lines = (chars + content_width - 1) / content_width;
        (lines as u16).max(1).min(3)
    }
}

/// Render all active toasts stacked in the top-right corner, newest on top.
pub fn render_toasts(f: &mut Frame, toasts: &[Toast], _theme: &Theme) {
    const TOAST_WIDTH: u16 = 56;
    const MARGIN_RIGHT: u16 = 1;
    const MARGIN_TOP: u16 = 1;
    // Inner content width: TOAST_WIDTH minus 2 borders minus 2 side padding
    const CONTENT_WIDTH: usize = (TOAST_WIDTH - 4) as usize;

    let area = f.area();
    let x = area.width.saturating_sub(TOAST_WIDTH + MARGIN_RIGHT);

    let mut y = MARGIN_TOP;
    for toast in toasts.iter().rev() {
        let content_lines = toast.content_lines(CONTENT_WIDTH);
        let toast_height = 2 + content_lines; // top border + lines + bottom border

        if y + toast_height > area.height {
            break;
        }

        let normal_color = match toast.level {
            ToastLevel::Success => Color::Green,
            ToastLevel::Warning => Color::Yellow,
            ToastLevel::Error => Color::Red,
        };
        let color = if toast.is_fading() {
            Color::Indexed(8)
        } else {
            normal_color
        };

        let toast_area = Rect { x, y, width: TOAST_WIDTH, height: toast_height };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color));

        let body = Paragraph::new(toast.message.as_str())
            .style(Style::default().fg(color))
            .block(block)
            .wrap(Wrap { trim: true });

        f.render_widget(Clear, toast_area);
        f.render_widget(body, toast_area);

        y += toast_height + 1; // gap between toasts
    }
}
