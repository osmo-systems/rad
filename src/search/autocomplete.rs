// Autocomplete engine for search queries

use crate::api::RadioBrowserClient;
use anyhow::Result;

pub struct AutocompleteData {
    pub countries: Vec<String>,
    pub languages: Vec<String>,
    pub tags: Vec<String>,
    pub codecs: Vec<String>,
    pub field_names: Vec<String>,
    pub order_values: Vec<String>,
    pub boolean_values: Vec<String>,
}

impl Default for AutocompleteData {
    fn default() -> Self {
        Self {
            countries: Vec::new(),
            languages: Vec::new(),
            tags: Vec::new(),
            codecs: vec![
                "MP3".to_string(),
                "AAC".to_string(),
                "AAC+".to_string(),
                "OGG".to_string(),
                "FLAC".to_string(),
            ],
            field_names: vec![
                "name".to_string(),
                "country".to_string(),
                "countrycode".to_string(),
                "state".to_string(),
                "language".to_string(),
                "tag".to_string(),
                "codec".to_string(),
                "bitrate_min".to_string(),
                "bitrate_max".to_string(),
                "order".to_string(),
                "reverse".to_string(),
                "hidebroken".to_string(),
                "is_https".to_string(),
                "page".to_string(),
            ],
            order_values: vec![
                "name".to_string(),
                "votes".to_string(),
                "clickcount".to_string(),
                "bitrate".to_string(),
                "changetimestamp".to_string(),
                "random".to_string(),
            ],
            boolean_values: vec![
                "true".to_string(),
                "false".to_string(),
            ],
        }
    }
}

impl AutocompleteData {
    /// Load autocomplete data from the API
    pub async fn load(api_client: &mut RadioBrowserClient) -> Result<Self> {
        let mut data = Self::default();

        // Load countries
        if let Ok(country_list) = api_client.get_countries().await {
            data.countries = country_list
                .into_iter()
                .map(|c| c.name)
                .collect();
        }

        // Load languages
        if let Ok(language_list) = api_client.get_languages().await {
            data.languages = language_list
                .into_iter()
                .map(|l| l.name)
                .collect();
        }

        // Load top 1000 tags
        if let Ok(tag_list) = api_client.get_tags(1000).await {
            data.tags = tag_list
                .into_iter()
                .map(|t| t.name)
                .collect();
        }

        Ok(data)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AutocompleteContext {
    FieldName,
    FieldValue(String),
    InvalidComma, // For country/language fields with commas (no autocomplete)
}

/// Detect the autocomplete context at the cursor position
pub fn detect_context(input: &str, cursor_pos: usize) -> AutocompleteContext {
    // Safety check
    if cursor_pos > input.len() {
        return AutocompleteContext::FieldName;
    }

    let before_cursor = &input[..cursor_pos];
    
    // Find the current field by looking backwards for space or start
    let current_field_start = before_cursor.rfind(' ').map(|i| i + 1).unwrap_or(0);
    let current_field = &before_cursor[current_field_start..];
    
    // Check for comma FIRST (before checking for '='), since "country=France," contains both
    if current_field.contains(',') {
        // We're after a comma, check if it's a multi-value field
        // Find the field name by looking for the '=' before the comma
        if let Some(equals_pos) = before_cursor.rfind('=') {
            let field_name = if let Some(field_start) = before_cursor[..equals_pos].rfind(' ') {
                before_cursor[field_start+1..equals_pos].trim().to_lowercase()
            } else {
                // '=' is at the start
                before_cursor[..equals_pos].trim().to_lowercase()
            };
            
            // Only 'tag' field supports multiple values (comma-separated)
            // country and language do NOT support multiple values
            if field_name == "tag" {
                AutocompleteContext::FieldValue(field_name)
            } else {
                // For country/language, comma is invalid - return InvalidComma to disable autocomplete
                AutocompleteContext::InvalidComma
            }
        } else {
            AutocompleteContext::FieldName
        }
    } else if let Some(equals_pos) = current_field.rfind('=') {
        // We're in a value context (after '=' but no comma)
        let field_name = current_field[..equals_pos].trim().to_lowercase();
        AutocompleteContext::FieldValue(field_name)
    } else {
        // We're in a field name context
        AutocompleteContext::FieldName
    }
}

/// Get autocomplete suggestions based on input and cursor position
pub fn get_suggestions(
    input: &str,
    cursor_pos: usize,
    data: &AutocompleteData,
) -> Vec<String> {
    if cursor_pos > input.len() {
        return Vec::new();
    }

    let before_cursor = &input[..cursor_pos];
    let context = detect_context(input, cursor_pos);
    
    match context {
        AutocompleteContext::InvalidComma => {
            // No suggestions for invalid comma in country/language fields
            Vec::new()
        }
        AutocompleteContext::FieldName => {
            // Get the current partial field name
            let current_field_start = before_cursor.rfind(' ').map(|i| i + 1).unwrap_or(0);
            let partial = before_cursor[current_field_start..].trim().to_lowercase();
            
            // Filter field names that start with the partial
            let mut suggestions: Vec<String> = data.field_names
                .iter()
                .filter(|field| field.to_lowercase().starts_with(&partial))
                .cloned()
                .collect();
            
            // Sort: exact matches first, then alphabetical
            suggestions.sort_by(|a, b| {
                let a_exact = a.to_lowercase() == partial;
                let b_exact = b.to_lowercase() == partial;
                match (a_exact, b_exact) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.cmp(b),
                }
            });
            
            // Limit to 300
            suggestions.truncate(300);
            suggestions
        }
        AutocompleteContext::FieldValue(field_name) => {
            // Get the current partial value
            let current_value_start = before_cursor.rfind(|c| c == '=' || c == ',')
                .map(|i| i + 1)
                .unwrap_or(0);
            let partial = before_cursor[current_value_start..].trim().to_lowercase();
            
            // Get the appropriate value list based on field name
            let value_list = match field_name.as_str() {
                "country" => &data.countries,
                "language" => &data.languages,
                "tag" => &data.tags,
                "codec" => &data.codecs,
                "order" => &data.order_values,
                "reverse" | "hidebroken" | "is_https" => &data.boolean_values,
                _ => return Vec::new(),
            };
            
            // Filter values that contain the partial (case-insensitive)
            let mut suggestions: Vec<String> = value_list
                .iter()
                .filter(|value| value.to_lowercase().contains(&partial))
                .cloned()
                .collect();
            
            // Sort: starts with partial first, then contains, then alphabetical
            suggestions.sort_by(|a, b| {
                let a_lower = a.to_lowercase();
                let b_lower = b.to_lowercase();
                let a_starts = a_lower.starts_with(&partial);
                let b_starts = b_lower.starts_with(&partial);
                
                match (a_starts, b_starts) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.cmp(b),
                }
            });
            
            // Limit to 300
            suggestions.truncate(300);
            suggestions
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_context_field_name() {
        assert_eq!(detect_context("", 0), AutocompleteContext::FieldName);
        assert_eq!(detect_context("co", 2), AutocompleteContext::FieldName);
        assert_eq!(detect_context("country=france ", 15), AutocompleteContext::FieldName);
    }

    #[test]
    fn test_detect_context_field_value() {
        let ctx = detect_context("country=", 8);
        assert_eq!(ctx, AutocompleteContext::FieldValue("country".to_string()));
        
        let ctx = detect_context("country=fr", 10);
        assert_eq!(ctx, AutocompleteContext::FieldValue("country".to_string()));
        
        let ctx = detect_context("country=france,", 15);
        assert_eq!(ctx, AutocompleteContext::InvalidComma); // trailing comma is invalid
    }

    #[test]
    fn test_get_suggestions_field_names() {
        let data = AutocompleteData::default();
        let suggestions = get_suggestions("co", 2, &data);
        
        assert!(suggestions.contains(&"country".to_string()));
        assert!(suggestions.contains(&"countrycode".to_string()));
        assert!(suggestions.contains(&"codec".to_string()));
    }

    #[test]
    fn test_get_suggestions_values() {
        let mut data = AutocompleteData::default();
        data.countries = vec!["france".to_string(), "germany".to_string(), "french guiana".to_string()];

        let suggestions = get_suggestions("country=fr", 10, &data);

        assert!(suggestions.contains(&"france".to_string()));
        assert!(suggestions.contains(&"french guiana".to_string()));
        assert!(!suggestions.contains(&"germany".to_string()));
    }

    #[test]
    fn test_detect_context_cursor_past_end_returns_field_name() {
        let ctx = detect_context("co", 100);
        assert_eq!(ctx, AutocompleteContext::FieldName);
    }

    #[test]
    fn test_detect_context_tag_comma_returns_field_value() {
        let input = "tag=jazz,";
        let ctx = detect_context(input, input.len());
        assert_eq!(ctx, AutocompleteContext::FieldValue("tag".to_string()));
    }

    #[test]
    fn test_detect_context_after_completed_field_is_field_name() {
        let input = "country=France t";
        let ctx = detect_context(input, input.len());
        assert_eq!(ctx, AutocompleteContext::FieldName);
    }

    #[test]
    fn test_detect_context_at_empty_value() {
        let input = "order=";
        let ctx = detect_context(input, input.len());
        assert_eq!(ctx, AutocompleteContext::FieldValue("order".to_string()));
    }

    #[test]
    fn test_get_suggestions_boolean_field() {
        let data = AutocompleteData::default();
        let input = "hidebroken=";
        let mut suggestions = get_suggestions(input, input.len(), &data);
        suggestions.sort();
        assert_eq!(suggestions, vec!["false".to_string(), "true".to_string()]);
    }

    #[test]
    fn test_get_suggestions_order_field_contains_all_values() {
        let data = AutocompleteData::default();
        let input = "order=";
        let suggestions = get_suggestions(input, input.len(), &data);
        for expected in ["votes", "name", "clickcount", "bitrate", "random"] {
            assert!(suggestions.contains(&expected.to_string()), "missing '{}'", expected);
        }
    }

    #[test]
    fn test_get_suggestions_unknown_field_returns_empty() {
        let data = AutocompleteData::default();
        let suggestions = get_suggestions("xyz=abc", 7, &data);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_get_suggestions_invalid_comma_returns_empty() {
        let data = AutocompleteData::default();
        let input = "country=france,";
        let suggestions = get_suggestions(input, input.len(), &data);
        assert!(suggestions.is_empty());
    }

    #[test]
    fn test_get_suggestions_starts_with_ranks_before_contains() {
        let mut data = AutocompleteData::default();
        // "mainstream" starts with "ma"; "drama" only contains "ma"
        data.tags = vec!["drama".to_string(), "mainstream".to_string()];
        let input = "tag=ma";
        let suggestions = get_suggestions(input, input.len(), &data);
        let mainstream_pos = suggestions.iter().position(|s| s == "mainstream").unwrap();
        let drama_pos = suggestions.iter().position(|s| s == "drama").unwrap();
        assert!(mainstream_pos < drama_pos);
    }

    #[test]
    fn test_get_suggestions_cursor_at_start_returns_all_fields() {
        let data = AutocompleteData::default();
        let suggestions = get_suggestions("", 0, &data);
        assert_eq!(suggestions.len(), data.field_names.len());
    }
}
