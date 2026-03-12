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
