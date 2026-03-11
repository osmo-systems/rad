use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

use crate::api::Station;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutovoteStation {
    pub uuid: String,
    pub name: String,
    pub url: String,
    pub country: String,
    pub codec: String,
    pub bitrate: i32,
    pub added_at: DateTime<Utc>,
}

impl From<&Station> for AutovoteStation {
    fn from(station: &Station) -> Self {
        Self {
            uuid: station.station_uuid.clone(),
            name: station.name.clone(),
            url: station.url_resolved.clone(),
            country: station.country.clone(),
            codec: station.codec.clone(),
            bitrate: station.bitrate,
            added_at: Utc::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AutovoteData {
    pub stations: Vec<AutovoteStation>,
}

pub struct AutovoteManager {
    file_path: PathBuf,
    data: AutovoteData,
}

impl AutovoteManager {
    pub fn new(data_dir: &PathBuf) -> Result<Self> {
        let file_path = data_dir.join("autovote.toml");

        let data = if file_path.exists() {
            debug!("Loading autovote list from: {:?}", file_path);
            let contents = fs::read_to_string(&file_path)
                .context("Failed to read autovote file")?;
            toml::from_str(&contents)
                .context("Failed to parse autovote file")?
        } else {
            info!("Creating new autovote file");
            AutovoteData { stations: Vec::new() }
        };

        Ok(Self { file_path, data })
    }

    pub fn add(&mut self, station: &Station) -> Result<()> {
        if self.contains(&station.station_uuid) {
            return Ok(());
        }
        info!("Adding station to autovote list: {}", station.name);
        self.data.stations.push(AutovoteStation::from(station));
        self.save()
    }

    pub fn remove(&mut self, uuid: &str) -> Result<()> {
        info!("Removing station from autovote list: {}", uuid);
        self.data.stations.retain(|s| s.uuid != uuid);
        self.save()
    }

    pub fn contains(&self, uuid: &str) -> bool {
        self.data.stations.iter().any(|s| s.uuid == uuid)
    }

    pub fn get_all(&self) -> &[AutovoteStation] {
        &self.data.stations
    }

    fn save(&self) -> Result<()> {
        debug!("Saving autovote list to: {:?}", self.file_path);
        let contents = toml::to_string_pretty(&self.data)
            .context("Failed to serialize autovote list")?;
        fs::write(&self.file_path, contents)
            .context("Failed to write autovote file")?;
        Ok(())
    }
}
