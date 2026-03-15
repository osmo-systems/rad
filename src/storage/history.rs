use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

use crate::api::Station;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryStation {
    pub uuid: String,
    pub name: String,
    pub url: String,
    pub country: String,
    pub tags: String,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub bitrate: i32,
    pub last_played: DateTime<Utc>,
    pub play_count: u32,
}

impl From<&Station> for HistoryStation {
    fn from(station: &Station) -> Self {
        Self {
            uuid: station.station_uuid.clone(),
            name: station.name.clone(),
            url: station.url_resolved.clone(),
            country: station.country.clone(),
            tags: station.tags.clone(),
            codec: station.codec.clone(),
            bitrate: station.bitrate,
            last_played: Utc::now(),
            play_count: 1,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryData {
    pub stations: Vec<HistoryStation>,
}

pub struct HistoryManager {
    file_path: PathBuf,
    history: HistoryData,
    max_entries: usize,
}

impl HistoryManager {
    pub fn new(data_dir: &PathBuf, max_entries: usize) -> Result<Self> {
        let file_path = data_dir.join("history.toml");
        
        let history = if file_path.exists() {
            debug!("Loading history from: {:?}", file_path);
            let contents = fs::read_to_string(&file_path)
                .context("Failed to read history file")?;
            toml::from_str(&contents)
                .context("Failed to parse history file")?
        } else {
            info!("Creating new history file");
            HistoryData {
                stations: Vec::new(),
            }
        };

        Ok(Self {
            file_path,
            history,
            max_entries,
        })
    }

    pub fn add(&mut self, station: &Station) -> Result<()> {
        info!("Adding station to history: {}", station.name);
        
        // Check if station already in history
        if let Some(existing) = self.history.stations.iter_mut().find(|s| s.uuid == station.station_uuid) {
            // Update existing entry
            existing.last_played = Utc::now();
            existing.play_count += 1;
        } else {
            // Add new entry
            self.history.stations.push(HistoryStation::from(station));
        }

        // Sort by last played (most recent first)
        self.history.stations.sort_by(|a, b| b.last_played.cmp(&a.last_played));

        // Limit number of entries
        if self.history.stations.len() > self.max_entries {
            self.history.stations.truncate(self.max_entries);
        }

        self.save()
    }

    pub fn get_all(&self) -> &[HistoryStation] {
        &self.history.stations
    }

    pub fn clear(&mut self) -> Result<()> {
        info!("Clearing history");
        self.history.stations.clear();
        self.save()
    }

    fn save(&self) -> Result<()> {
        debug!("Saving history to: {:?}", self.file_path);
        let contents = toml::to_string_pretty(&self.history)
            .context("Failed to serialize history")?;
        fs::write(&self.file_path, contents)
            .context("Failed to write history file")?;
        Ok(())
    }
}
