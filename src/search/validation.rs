use super::types::SearchQuery;

pub const VALID_FIELDS: &[&str] = &[
    "name",
    "country",
    "countrycode",
    "state",
    "language",
    "tag",
    "codec",
    "bitrate_min",
    "bitrate_max",
    "order",
    "reverse",
    "hidebroken",
    "is_https",
    "page",
];

pub const VALID_ORDER_VALUES: &[&str] = &[
    "name",
    "votes",
    "clickcount",
    "bitrate",
    "changetimestamp",
    "random",
];

pub fn validate_field(field: &str) -> bool {
    VALID_FIELDS.contains(&field)
}

pub fn is_default_query(query: &SearchQuery) -> bool {
    let default = SearchQuery::default();
    query.name.is_none()
        && query.country.is_none()
        && query.countrycode.is_none()
        && query.state.is_none()
        && query.language.is_none()
        && query.tags.is_none()
        && query.codec.is_none()
        && query.bitrate_min.is_none()
        && query.bitrate_max.is_none()
        && query.is_https.is_none()
        && query.order == default.order
        && query.reverse == default.reverse
        && query.hidebroken == default.hidebroken
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_all_known_fields() {
        let known = [
            "name", "country", "countrycode", "state", "language",
            "tag", "codec", "bitrate_min", "bitrate_max", "order",
            "reverse", "hidebroken", "is_https", "page",
        ];
        for field in known {
            assert!(validate_field(field), "expected '{}' to be valid", field);
        }
    }

    #[test]
    fn test_validate_unknown_fields() {
        assert!(!validate_field(""));
        assert!(!validate_field("xyz"));
        assert!(!validate_field("tag_name"));
        assert!(!validate_field("bitrate"));  // bitrate_min/max exist, but not bare "bitrate"
    }

    #[test]
    fn test_validate_field_is_case_sensitive() {
        assert!(!validate_field("NAME"));
        assert!(!validate_field("Country"));
        assert!(!validate_field("TAG"));
    }

    #[test]
    fn test_is_default_query_on_default() {
        assert!(is_default_query(&SearchQuery::default()));
    }

    #[test]
    fn test_is_default_query_each_filter_field() {
        let cases: Vec<fn(&mut SearchQuery)> = vec![
            |q| q.name = Some("jazz".into()),
            |q| q.country = Some("France".into()),
            |q| q.countrycode = Some("FR".into()),
            |q| q.state = Some("Ile-de-France".into()),
            |q| q.language = Some("french".into()),
            |q| q.tags = Some(vec!["jazz".into()]),
            |q| q.codec = Some("MP3".into()),
            |q| q.bitrate_min = Some(128),
            |q| q.bitrate_max = Some(320),
            |q| q.is_https = Some(true),
            |q| q.order = Some("name".into()),
            |q| q.reverse = Some(false),
            |q| q.hidebroken = Some(false),
        ];
        for modify in cases {
            let mut q = SearchQuery::default();
            modify(&mut q);
            assert!(!is_default_query(&q));
        }
    }

    #[test]
    fn test_is_default_query_pagination_offset_ignored() {
        let mut q = SearchQuery::default();
        q.offset = 24;
        // offset/limit changes don't affect "is default query" check
        assert!(is_default_query(&q));
    }
}
