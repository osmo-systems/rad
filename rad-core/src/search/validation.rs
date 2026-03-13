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
