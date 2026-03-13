use std::fmt;

/// Represents a parsed search query
#[derive(Debug, Clone, PartialEq)]
pub struct SearchQuery {
    pub name: Option<String>,
    pub country: Option<String>,
    pub countrycode: Option<String>,
    pub state: Option<String>,
    pub language: Option<String>,
    pub tags: Option<Vec<String>>,
    pub codec: Option<String>,
    pub bitrate_min: Option<u32>,
    pub bitrate_max: Option<u32>,
    pub order: Option<String>,
    pub reverse: Option<bool>,
    pub hidebroken: Option<bool>,
    pub is_https: Option<bool>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        Self {
            name: None,
            country: None,
            countrycode: None,
            state: None,
            language: None,
            tags: None,
            codec: None,
            bitrate_min: None,
            bitrate_max: None,
            order: Some("votes".to_string()),
            reverse: Some(true),
            hidebroken: Some(true),
            is_https: None,
            limit: 12,
            offset: 0,
        }
    }
}

impl SearchQuery {
    pub fn next_page(&mut self) {
        self.offset += self.limit;
    }

    pub fn prev_page(&mut self) {
        self.offset = self.offset.saturating_sub(self.limit);
    }

    pub fn current_page(&self) -> usize {
        (self.offset / self.limit) + 1
    }

    pub fn reset_pagination(&mut self) {
        self.offset = 0;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParseError {
    UnknownField(String),
    InvalidSyntax(String),
    InvalidValue {
        field: String,
        value: String,
        reason: String,
    },
    MissingEquals(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseError::UnknownField(field) => write!(f, "Unknown field: '{}'", field),
            ParseError::InvalidSyntax(msg) => write!(f, "Invalid syntax: {}", msg),
            ParseError::InvalidValue { field, value, reason } => {
                write!(f, "Invalid value '{}' for field '{}': {}", value, field, reason)
            }
            ParseError::MissingEquals(field) => {
                write!(f, "Missing '=' after field '{}'", field)
            }
        }
    }
}

impl std::error::Error for ParseError {}
