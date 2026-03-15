use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum StartupTab {
    Search,
    Favorites,
    History,
}

impl Default for StartupTab {
    fn default() -> Self {
        StartupTab::Search
    }
}

impl StartupTab {
    pub fn label(&self) -> &'static str {
        match self {
            StartupTab::Search => "Search",
            StartupTab::Favorites => "Favorites",
            StartupTab::History => "History",
        }
    }

    pub fn cycle_next(&self) -> Self {
        match self {
            StartupTab::Search => StartupTab::Favorites,
            StartupTab::Favorites => StartupTab::History,
            StartupTab::History => StartupTab::Search,
        }
    }

    pub fn cycle_prev(&self) -> Self {
        match self {
            StartupTab::Search => StartupTab::History,
            StartupTab::Favorites => StartupTab::Search,
            StartupTab::History => StartupTab::Favorites,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DefaultSearchOrder {
    Votes,
    Name,
    ClickCount,
    Bitrate,
    Random,
}

impl Default for DefaultSearchOrder {
    fn default() -> Self {
        DefaultSearchOrder::Votes
    }
}

impl DefaultSearchOrder {
    pub fn as_api_str(&self) -> &'static str {
        match self {
            DefaultSearchOrder::Votes => "votes",
            DefaultSearchOrder::Name => "name",
            DefaultSearchOrder::ClickCount => "clickcount",
            DefaultSearchOrder::Bitrate => "bitrate",
            DefaultSearchOrder::Random => "random",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            DefaultSearchOrder::Votes => "Votes",
            DefaultSearchOrder::Name => "Name",
            DefaultSearchOrder::ClickCount => "Click Count",
            DefaultSearchOrder::Bitrate => "Bitrate",
            DefaultSearchOrder::Random => "Random",
        }
    }

    pub fn cycle_next(&self) -> Self {
        match self {
            DefaultSearchOrder::Votes => DefaultSearchOrder::Name,
            DefaultSearchOrder::Name => DefaultSearchOrder::ClickCount,
            DefaultSearchOrder::ClickCount => DefaultSearchOrder::Bitrate,
            DefaultSearchOrder::Bitrate => DefaultSearchOrder::Random,
            DefaultSearchOrder::Random => DefaultSearchOrder::Votes,
        }
    }

    pub fn cycle_prev(&self) -> Self {
        match self {
            DefaultSearchOrder::Votes => DefaultSearchOrder::Random,
            DefaultSearchOrder::Name => DefaultSearchOrder::Votes,
            DefaultSearchOrder::ClickCount => DefaultSearchOrder::Name,
            DefaultSearchOrder::Bitrate => DefaultSearchOrder::ClickCount,
            DefaultSearchOrder::Random => DefaultSearchOrder::Bitrate,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub cache_duration_secs: u64,
    pub max_history_entries: usize,
    pub default_volume: f32,
    pub station_limit: usize,

    // User preferences
    #[serde(default)]
    pub startup_tab: StartupTab,
    #[serde(default)]
    pub default_search_order: DefaultSearchOrder,
    #[serde(default)]
    pub play_at_startup: bool,
    #[serde(default)]
    pub auto_vote_favorites: bool,
    #[serde(default = "default_true")]
    pub show_logo: bool,
    #[serde(default = "default_toast_duration")]
    pub toast_duration_secs: u64,

    // Session state
    #[serde(default)]
    pub last_volume: Option<f32>,
    #[serde(default)]
    pub last_station_name: Option<String>,
    #[serde(default)]
    pub last_station_url: Option<String>,
}

fn default_true() -> bool {
    true
}

fn default_toast_duration() -> u64 {
    3
}

pub const TOAST_DURATION_OPTIONS: &[u64] = &[0, 1, 2, 3, 5, 10];

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_duration_secs: 3600, // 1 hour
            max_history_entries: 50,
            default_volume: 0.5,
            station_limit: 100,
            startup_tab: StartupTab::default(),
            default_search_order: DefaultSearchOrder::default(),
            play_at_startup: false,
            auto_vote_favorites: false,
            show_logo: true,
            toast_duration_secs: 3,
            last_volume: None,
            last_station_name: None,
            last_station_url: None,
        }
    }
}

impl Config {
    pub fn load(data_dir: &PathBuf) -> Result<Self> {
        let config_file = data_dir.join("config.toml");

        if config_file.exists() {
            let contents =
                fs::read_to_string(&config_file).context("Failed to read config file")?;
            toml::from_str(&contents).context("Failed to parse config file")
        } else {
            info!("Creating default config");
            let config = Self::default();
            config.save(data_dir)?;
            Ok(config)
        }
    }

    pub fn save(&self, data_dir: &PathBuf) -> Result<()> {
        let config_file = data_dir.join("config.toml");
        let contents = toml::to_string_pretty(self).context("Failed to serialize config")?;
        fs::write(&config_file, contents).context("Failed to write config file")?;
        Ok(())
    }

    pub fn update_session_state(
        &mut self,
        volume: f32,
        station_name: Option<String>,
        station_url: Option<String>,
    ) {
        self.last_volume = Some(volume);
        self.last_station_name = station_name;
        self.last_station_url = station_url;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_startup_tab_labels() {
        assert_eq!(StartupTab::Search.label(), "Search");
        assert_eq!(StartupTab::Favorites.label(), "Favorites");
        assert_eq!(StartupTab::History.label(), "History");
    }

    #[test]
    fn test_startup_tab_cycle_next() {
        assert_eq!(StartupTab::Search.cycle_next(), StartupTab::Favorites);
        assert_eq!(StartupTab::Favorites.cycle_next(), StartupTab::History);
        assert_eq!(StartupTab::History.cycle_next(), StartupTab::Search);
    }

    #[test]
    fn test_startup_tab_cycle_prev() {
        assert_eq!(StartupTab::Search.cycle_prev(), StartupTab::History);
        assert_eq!(StartupTab::Favorites.cycle_prev(), StartupTab::Search);
        assert_eq!(StartupTab::History.cycle_prev(), StartupTab::Favorites);
    }

    #[test]
    fn test_startup_tab_next_then_prev_is_identity() {
        for tab in [StartupTab::Search, StartupTab::Favorites, StartupTab::History] {
            assert_eq!(tab.cycle_next().cycle_prev(), tab);
            assert_eq!(tab.cycle_prev().cycle_next(), tab);
        }
    }

    #[test]
    fn test_default_search_order_as_api_str() {
        assert_eq!(DefaultSearchOrder::Votes.as_api_str(), "votes");
        assert_eq!(DefaultSearchOrder::Name.as_api_str(), "name");
        assert_eq!(DefaultSearchOrder::ClickCount.as_api_str(), "clickcount");
        assert_eq!(DefaultSearchOrder::Bitrate.as_api_str(), "bitrate");
        assert_eq!(DefaultSearchOrder::Random.as_api_str(), "random");
    }

    #[test]
    fn test_default_search_order_labels() {
        assert_eq!(DefaultSearchOrder::Votes.label(), "Votes");
        assert_eq!(DefaultSearchOrder::Name.label(), "Name");
        assert_eq!(DefaultSearchOrder::ClickCount.label(), "Click Count");
        assert_eq!(DefaultSearchOrder::Bitrate.label(), "Bitrate");
        assert_eq!(DefaultSearchOrder::Random.label(), "Random");
    }

    #[test]
    fn test_default_search_order_cycle_next_full_rotation() {
        assert_eq!(DefaultSearchOrder::Votes.cycle_next(), DefaultSearchOrder::Name);
        assert_eq!(DefaultSearchOrder::Name.cycle_next(), DefaultSearchOrder::ClickCount);
        assert_eq!(DefaultSearchOrder::ClickCount.cycle_next(), DefaultSearchOrder::Bitrate);
        assert_eq!(DefaultSearchOrder::Bitrate.cycle_next(), DefaultSearchOrder::Random);
        assert_eq!(DefaultSearchOrder::Random.cycle_next(), DefaultSearchOrder::Votes);
    }

    #[test]
    fn test_default_search_order_cycle_prev_full_rotation() {
        assert_eq!(DefaultSearchOrder::Votes.cycle_prev(), DefaultSearchOrder::Random);
        assert_eq!(DefaultSearchOrder::Name.cycle_prev(), DefaultSearchOrder::Votes);
        assert_eq!(DefaultSearchOrder::ClickCount.cycle_prev(), DefaultSearchOrder::Name);
        assert_eq!(DefaultSearchOrder::Bitrate.cycle_prev(), DefaultSearchOrder::ClickCount);
        assert_eq!(DefaultSearchOrder::Random.cycle_prev(), DefaultSearchOrder::Bitrate);
    }

    #[test]
    fn test_default_search_order_next_then_prev_is_identity() {
        let variants = [
            DefaultSearchOrder::Votes,
            DefaultSearchOrder::Name,
            DefaultSearchOrder::ClickCount,
            DefaultSearchOrder::Bitrate,
            DefaultSearchOrder::Random,
        ];
        for order in variants {
            assert_eq!(order.cycle_next().cycle_prev(), order);
            assert_eq!(order.cycle_prev().cycle_next(), order);
        }
    }

    #[test]
    fn test_config_update_session_state() {
        let mut config = Config::default();
        config.update_session_state(
            0.8,
            Some("Jazz FM".to_string()),
            Some("http://stream.example.com".to_string()),
        );
        assert_eq!(config.last_volume, Some(0.8));
        assert_eq!(config.last_station_name, Some("Jazz FM".to_string()));
        assert_eq!(config.last_station_url, Some("http://stream.example.com".to_string()));
    }

    #[test]
    fn test_config_update_session_state_clears_station() {
        let mut config = Config::default();
        config.update_session_state(0.5, Some("s".to_string()), Some("u".to_string()));
        config.update_session_state(0.3, None, None);
        assert_eq!(config.last_volume, Some(0.3));
        assert_eq!(config.last_station_name, None);
        assert_eq!(config.last_station_url, None);
    }

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert_eq!(config.default_volume, 0.5);
        assert!(!config.play_at_startup);
        assert!(!config.auto_vote_favorites);
        assert!(config.show_logo);
        assert_eq!(config.toast_duration_secs, 3);
        assert_eq!(config.max_history_entries, 50);
    }
}

pub fn get_data_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_dir()
        .context("Failed to get data directory")?
        .join("radm");

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir).context("Failed to create data directory")?;
    }

    Ok(data_dir)
}

/// Clean up log files older than the specified number of days
pub fn cleanup_old_logs(data_dir: &PathBuf, max_age_days: u64) -> Result<()> {
    let max_age_secs = max_age_days * 24 * 60 * 60;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("Failed to get current time")?
        .as_secs();

    // Read directory entries
    let entries = fs::read_dir(data_dir).context("Failed to read data directory")?;

    let mut deleted_count = 0;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        // Only process files that match the log pattern
        if path.is_file() {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                // Match log files like "radm.log.2026-03-01" but not the current "radm.log"
                if filename.starts_with("radm.log.") {
                    // Get file metadata to check age
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(duration) = modified.duration_since(UNIX_EPOCH) {
                                let file_age_secs = now.saturating_sub(duration.as_secs());

                                if file_age_secs > max_age_secs {
                                    match fs::remove_file(&path) {
                                        Ok(_) => {
                                            info!("Deleted old log file: {}", filename);
                                            deleted_count += 1;
                                        }
                                        Err(e) => {
                                            tracing::warn!(
                                                "Failed to delete old log file {}: {}",
                                                filename,
                                                e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    if deleted_count > 0 {
        info!("Cleaned up {} old log file(s)", deleted_count);
    }

    Ok(())
}
