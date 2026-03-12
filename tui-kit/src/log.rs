use std::time::Instant;

use ratatui::style::Color;

/// Severity level for a log entry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    /// The display color associated with this level.
    pub fn color(self) -> Color {
        match self {
            LogLevel::Debug => Color::DarkGray,
            LogLevel::Info => Color::White,
            LogLevel::Warning => Color::Yellow,
            LogLevel::Error => Color::Red,
        }
    }
}

/// A single timestamped log entry with a severity level.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
    /// Wall-clock time of creation, used to evict entries older than 24 h.
    pub created_at: Instant,
}
