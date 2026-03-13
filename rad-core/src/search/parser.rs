pub use super::types::{ParseError, SearchQuery};
pub use super::validation::{is_default_query, validate_field};

use super::validation::VALID_ORDER_VALUES;

fn tokenize_query(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_quotes = false;

    for ch in input.chars() {
        match ch {
            '"' => {
                in_quotes = !in_quotes;
            }
            ' ' if !in_quotes => {
                if !current_token.is_empty() {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
            }
            _ => {
                current_token.push(ch);
            }
        }
    }

    if !current_token.is_empty() {
        tokens.push(current_token);
    }

    tokens
}

/// Parse a search query string.
/// Format: `field=value1,value2 field2=value3`
/// Values with spaces should be quoted: `field="value with spaces"`
pub fn parse_query(input: &str) -> Result<SearchQuery, ParseError> {
    let input = input.trim();

    if input.is_empty() {
        return Ok(SearchQuery::default());
    }

    let mut query = SearchQuery::default();
    let pairs = tokenize_query(input);
    let mut bare_words: Vec<String> = Vec::new();

    for pair in pairs {
        let parts: Vec<&str> = pair.splitn(2, '=').collect();

        if parts.len() != 2 {
            bare_words.push(parts[0].to_string());
            continue;
        }

        let field = parts[0].trim().to_lowercase();
        let value = parts[1].trim();

        if !validate_field(&field) {
            return Err(ParseError::UnknownField(field));
        }

        match field.as_str() {
            "name" => {
                query.name = Some(value.to_string());
            }
            "country" => {
                if value.trim().is_empty() {
                    return Err(ParseError::InvalidValue {
                        field: "country".to_string(),
                        value: value.to_string(),
                        reason: "empty value".to_string(),
                    });
                }
                if value.contains(',') {
                    return Err(ParseError::InvalidValue {
                        field: "country".to_string(),
                        value: value.to_string(),
                        reason: "multiple countries not supported (use single country only)"
                            .to_string(),
                    });
                }
                query.country = Some(value.trim().to_string());
            }
            "countrycode" => {
                if value.len() != 2 {
                    return Err(ParseError::InvalidValue {
                        field: "countrycode".to_string(),
                        value: value.to_string(),
                        reason: "must be 2-letter code".to_string(),
                    });
                }
                query.countrycode = Some(value.to_uppercase());
            }
            "state" => {
                query.state = Some(value.to_string());
            }
            "language" => {
                if value.trim().is_empty() {
                    return Err(ParseError::InvalidValue {
                        field: "language".to_string(),
                        value: value.to_string(),
                        reason: "empty value".to_string(),
                    });
                }
                if value.contains(',') {
                    return Err(ParseError::InvalidValue {
                        field: "language".to_string(),
                        value: value.to_string(),
                        reason: "multiple languages not supported (use single language only)"
                            .to_string(),
                    });
                }
                query.language = Some(value.trim().to_string());
            }
            "tag" => {
                let tags: Vec<String> = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if tags.is_empty() {
                    return Err(ParseError::InvalidValue {
                        field: "tag".to_string(),
                        value: value.to_string(),
                        reason: "empty value".to_string(),
                    });
                }
                query.tags = Some(tags);
            }
            "codec" => {
                query.codec = Some(value.to_string());
            }
            "bitrate_min" => {
                let bitrate = value.parse::<u32>().map_err(|_| ParseError::InvalidValue {
                    field: "bitrate_min".to_string(),
                    value: value.to_string(),
                    reason: "must be a number".to_string(),
                })?;
                query.bitrate_min = Some(bitrate);
            }
            "bitrate_max" => {
                let bitrate = value.parse::<u32>().map_err(|_| ParseError::InvalidValue {
                    field: "bitrate_max".to_string(),
                    value: value.to_string(),
                    reason: "must be a number".to_string(),
                })?;
                query.bitrate_max = Some(bitrate);
            }
            "order" => {
                let order_value = value.to_lowercase();
                if !VALID_ORDER_VALUES.contains(&order_value.as_str()) {
                    return Err(ParseError::InvalidValue {
                        field: "order".to_string(),
                        value: value.to_string(),
                        reason: format!("must be one of: {}", VALID_ORDER_VALUES.join(", ")),
                    });
                }
                query.order = Some(order_value);
            }
            "reverse" => {
                let v = value.to_lowercase();
                match v.as_str() {
                    "true" => query.reverse = Some(true),
                    "false" => query.reverse = Some(false),
                    _ => {
                        return Err(ParseError::InvalidValue {
                            field: "reverse".to_string(),
                            value: value.to_string(),
                            reason: "must be 'true' or 'false'".to_string(),
                        })
                    }
                }
            }
            "hidebroken" => {
                let v = value.to_lowercase();
                match v.as_str() {
                    "true" => query.hidebroken = Some(true),
                    "false" => query.hidebroken = Some(false),
                    _ => {
                        return Err(ParseError::InvalidValue {
                            field: "hidebroken".to_string(),
                            value: value.to_string(),
                            reason: "must be 'true' or 'false'".to_string(),
                        })
                    }
                }
            }
            "is_https" => {
                let v = value.to_lowercase();
                match v.as_str() {
                    "true" => query.is_https = Some(true),
                    "false" => query.is_https = Some(false),
                    _ => {
                        return Err(ParseError::InvalidValue {
                            field: "is_https".to_string(),
                            value: value.to_string(),
                            reason: "must be 'true' or 'false'".to_string(),
                        })
                    }
                }
            }
            "page" => {
                match value.parse::<usize>() {
                    Ok(page_num) if page_num > 0 => {
                        query.offset = (page_num - 1) * query.limit;
                    }
                    _ => {
                        return Err(ParseError::InvalidValue {
                            field: "page".to_string(),
                            value: value.to_string(),
                            reason: "must be a positive integer".to_string(),
                        })
                    }
                }
            }
            _ => unreachable!(),
        }
    }

    if !bare_words.is_empty() && query.name.is_none() {
        query.name = Some(bare_words.join(" "));
    }

    Ok(query)
}

/// Format a SearchQuery back to a query string
pub fn format_query(query: &SearchQuery) -> String {
    let mut parts = Vec::new();

    if let Some(name) = &query.name {
        parts.push(format!("name={}", name));
    }
    if let Some(country) = &query.country {
        parts.push(format!("country={}", country));
    }
    if let Some(countrycode) = &query.countrycode {
        parts.push(format!("countrycode={}", countrycode));
    }
    if let Some(state) = &query.state {
        parts.push(format!("state={}", state));
    }
    if let Some(language) = &query.language {
        parts.push(format!("language={}", language));
    }
    if let Some(tags) = &query.tags {
        let joined: String = tags.join(",");
        parts.push(format!("tag={}", joined));
    }
    if let Some(codec) = &query.codec {
        parts.push(format!("codec={}", codec));
    }
    if let Some(bitrate_min) = query.bitrate_min {
        parts.push(format!("bitrate_min={}", bitrate_min));
    }
    if let Some(bitrate_max) = query.bitrate_max {
        parts.push(format!("bitrate_max={}", bitrate_max));
    }
    if let Some(order) = &query.order {
        if order != "votes" {
            parts.push(format!("order={}", order));
        }
    }
    if let Some(reverse) = query.reverse {
        if !reverse {
            parts.push(format!("reverse=false"));
        }
    }
    if let Some(hidebroken) = query.hidebroken {
        if !hidebroken {
            parts.push(format!("hidebroken=false"));
        }
    }
    if let Some(is_https) = query.is_https {
        parts.push(format!("is_https={}", is_https));
    }

    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_query() {
        let query = parse_query("").unwrap();
        assert!(is_default_query(&query));
    }

    #[test]
    fn test_parse_simple_query() {
        let query = parse_query("country=france").unwrap();
        assert_eq!(query.country, Some("france".to_string()));
    }

    #[test]
    fn test_parse_multiple_values() {
        let result = parse_query("country=france,germany");
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }

    #[test]
    fn test_parse_multiple_fields() {
        let query = parse_query("country=france tag=jazz").unwrap();
        assert_eq!(query.country, Some("france".to_string()));
        assert_eq!(query.tags, Some(vec!["jazz".to_string()]));
    }

    #[test]
    fn test_parse_numeric_field() {
        let query = parse_query("bitrate_min=128").unwrap();
        assert_eq!(query.bitrate_min, Some(128));
    }

    #[test]
    fn test_parse_boolean_field() {
        let query = parse_query("hidebroken=true").unwrap();
        assert_eq!(query.hidebroken, Some(true));

        let query = parse_query("hidebroken=false").unwrap();
        assert_eq!(query.hidebroken, Some(false));
    }

    #[test]
    fn test_invalid_field() {
        let result = parse_query("countrx=france");
        assert!(matches!(result, Err(ParseError::UnknownField(_))));
    }

    #[test]
    fn test_bare_word_as_name() {
        let query = parse_query("jazz").unwrap();
        assert_eq!(query.name, Some("jazz".to_string()));
    }

    #[test]
    fn test_bare_words_joined_as_name() {
        let query = parse_query("jazz radio").unwrap();
        assert_eq!(query.name, Some("jazz radio".to_string()));
    }

    #[test]
    fn test_bare_word_does_not_override_explicit_name() {
        let query = parse_query("name=blues extra").unwrap();
        assert_eq!(query.name, Some("blues".to_string()));
    }

    #[test]
    fn test_invalid_boolean() {
        let result = parse_query("hidebroken=yes");
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }

    #[test]
    fn test_invalid_number() {
        let result = parse_query("bitrate_min=abc");
        assert!(matches!(result, Err(ParseError::InvalidValue { .. })));
    }

    #[test]
    fn test_format_query() {
        let mut query = SearchQuery::default();
        query.country = Some("france".to_string());
        query.tags = Some(vec!["jazz".to_string()]);

        let formatted = format_query(&query);
        assert!(formatted.contains("country=france"));
        assert!(formatted.contains("tag=jazz"));
    }

    #[test]
    fn test_is_default_query() {
        let query = SearchQuery::default();
        assert!(is_default_query(&query));

        let mut query = SearchQuery::default();
        query.country = Some("france".to_string());
        assert!(!is_default_query(&query));
    }

    #[test]
    fn test_pagination() {
        let mut query = SearchQuery::default();
        assert_eq!(query.current_page(), 1);

        query.next_page();
        assert_eq!(query.current_page(), 2);
        assert_eq!(query.offset, query.limit);

        query.prev_page();
        assert_eq!(query.current_page(), 1);
        assert_eq!(query.offset, 0);
    }
}
