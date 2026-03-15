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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_current_page_initial() {
        assert_eq!(SearchQuery::default().current_page(), 1);
    }

    #[test]
    fn test_next_page_increments() {
        let mut q = SearchQuery::default();
        q.next_page();
        assert_eq!(q.current_page(), 2);
        assert_eq!(q.offset, q.limit);
    }

    #[test]
    fn test_next_page_multiple_times() {
        let mut q = SearchQuery::default();
        q.next_page();
        q.next_page();
        q.next_page();
        assert_eq!(q.current_page(), 4);
        assert_eq!(q.offset, q.limit * 3);
    }

    #[test]
    fn test_prev_page_at_first_page_does_not_underflow() {
        let mut q = SearchQuery::default();
        q.prev_page();
        assert_eq!(q.offset, 0);
        assert_eq!(q.current_page(), 1);
    }

    #[test]
    fn test_prev_page_returns_to_previous() {
        let mut q = SearchQuery::default();
        q.next_page();
        q.next_page();
        q.prev_page();
        assert_eq!(q.current_page(), 2);
    }

    #[test]
    fn test_reset_pagination() {
        let mut q = SearchQuery::default();
        q.next_page();
        q.next_page();
        q.reset_pagination();
        assert_eq!(q.offset, 0);
        assert_eq!(q.current_page(), 1);
    }

    #[test]
    fn test_current_page_with_custom_limit() {
        let mut q = SearchQuery::default();
        q.limit = 20;
        q.offset = 40;
        assert_eq!(q.current_page(), 3); // (40/20)+1
    }

    #[test]
    fn test_parse_error_display_unknown_field() {
        let e = ParseError::UnknownField("foo".to_string());
        assert_eq!(e.to_string(), "Unknown field: 'foo'");
    }

    #[test]
    fn test_parse_error_display_invalid_value() {
        let e = ParseError::InvalidValue {
            field: "country".to_string(),
            value: "a,b".to_string(),
            reason: "multiple values not allowed".to_string(),
        };
        let s = e.to_string();
        assert!(s.contains("country"));
        assert!(s.contains("a,b"));
        assert!(s.contains("multiple values not allowed"));
    }

    #[test]
    fn test_parse_error_display_missing_equals() {
        let e = ParseError::MissingEquals("country".to_string());
        assert!(e.to_string().contains("country"));
        assert!(e.to_string().contains("="));
    }
}
