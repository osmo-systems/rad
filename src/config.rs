use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub cache_duration_secs: u64,
    pub max_history_entries: usize,
    pub default_volume: f32,
    pub station_limit: usize,
    
    // Session state
    #[serde(default)]
    pub last_volume: Option<f32>,
    #[serde(default)]
    pub last_station_name: Option<String>,
    #[serde(default)]
    pub last_station_url: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            cache_duration_secs: 3600, // 1 hour
            max_history_entries: 50,
            default_volume: 0.5,
            station_limit: 100,
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
            let contents = fs::read_to_string(&config_file)
                .context("Failed to read config file")?;
            toml::from_str(&contents)
                .context("Failed to parse config file")
        } else {
            info!("Creating default config");
            let config = Self::default();
            config.save(data_dir)?;
            Ok(config)
        }
    }

    pub fn save(&self, data_dir: &PathBuf) -> Result<()> {
        let config_file = data_dir.join("config.toml");
        let contents = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(&config_file, contents)
            .context("Failed to write config file")?;
        Ok(())
    }
    
    pub fn update_session_state(&mut self, volume: f32, station_name: Option<String>, station_url: Option<String>) {
        self.last_volume = Some(volume);
        self.last_station_name = station_name;
        self.last_station_url = station_url;
    }
}

pub fn get_data_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_dir()
        .context("Failed to get data directory")?
        .join("web-radio");

    if !data_dir.exists() {
        fs::create_dir_all(&data_dir)
            .context("Failed to create data directory")?;
    }

    Ok(data_dir)
}
