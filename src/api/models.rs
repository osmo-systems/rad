use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    #[serde(rename = "stationuuid")]
    pub station_uuid: String,
    #[serde(rename = "changeuuid")]
    pub change_uuid: String,
    pub name: String,
    pub url: String,
    #[serde(rename = "url_resolved")]
    pub url_resolved: String,
    pub homepage: String,
    pub favicon: String,
    pub tags: String,
    pub country: String,
    #[serde(rename = "countrycode")]
    pub country_code: String,
    pub state: String,
    pub language: String,
    #[serde(rename = "languagecodes")]
    pub language_codes: String,
    pub votes: i32,
    pub codec: String,
    pub bitrate: i32,
    pub hls: i32,
    #[serde(rename = "lastcheckok")]
    pub last_check_ok: i32,
    #[serde(rename = "lastchecktime")]
    pub last_check_time: String,
    #[serde(rename = "lastcheckoktime")]
    pub last_check_ok_time: String,
    #[serde(rename = "clicktimestamp")]
    pub click_timestamp: String,
    #[serde(rename = "clickcount")]
    pub click_count: i32,
    #[serde(rename = "clicktrend")]
    pub click_trend: i32,
}

impl Station {
    pub fn is_online(&self) -> bool {
        self.last_check_ok == 1
    }

    pub fn get_tags(&self) -> Vec<String> {
        self.tags
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn get_languages(&self) -> Vec<String> {
        self.language
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    pub fn format_bitrate(&self) -> String {
        if self.bitrate > 0 {
            format!("{} kbps", self.bitrate)
        } else {
            "—".to_string()
        }
    }

    pub fn format_codec(&self) -> String {
        if self.codec.is_empty() {
            "—".to_string()
        } else {
            self.codec.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Country {
    pub name: String,
    #[serde(rename = "iso_3166_1")]
    pub iso_3166_1: String,
    #[serde(rename = "stationcount")]
    pub station_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    pub name: String,
    #[serde(rename = "stationcount")]
    pub station_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub name: String,
    #[serde(rename = "iso_639")]
    pub iso_639: Option<String>,
    #[serde(rename = "stationcount")]
    pub station_count: i32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ClickResponse {
    pub ok: bool,
    pub message: String,
    #[serde(rename = "stationuuid")]
    pub station_uuid: String,
    pub name: String,
    pub url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VoteResponse {
    pub ok: bool,
    pub message: String,
}
