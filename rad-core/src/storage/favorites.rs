use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

use crate::api::Station;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FavoriteStation {
    pub uuid: String,
    pub name: String,
    pub url: String,
    pub country: String,
    pub tags: String,
    #[serde(default)]
    pub codec: String,
    #[serde(default)]
    pub bitrate: i32,
    pub added_at: DateTime<Utc>,
}

impl From<&Station> for FavoriteStation {
    fn from(station: &Station) -> Self {
        Self {
            uuid: station.station_uuid.clone(),
            name: station.name.clone(),
            url: station.url_resolved.clone(),
            country: station.country.clone(),
            tags: station.tags.clone(),
            codec: station.codec.clone(),
            bitrate: station.bitrate,
            added_at: Utc::now(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FavoritesData {
    pub stations: Vec<FavoriteStation>,
}

pub struct FavoritesManager {
    file_path: PathBuf,
    favorites: FavoritesData,
}

impl FavoritesManager {
    pub fn new(data_dir: &PathBuf) -> Result<Self> {
        let file_path = data_dir.join("favorites.toml");
        
        let favorites = if file_path.exists() {
            debug!("Loading favorites from: {:?}", file_path);
            let contents = fs::read_to_string(&file_path)
                .context("Failed to read favorites file")?;
            match toml::from_str(&contents) {
                Ok(data) => data,
                Err(e) => {
                    tracing::warn!("Corrupted favorites file, removing: {}", e);
                    let _ = fs::remove_file(&file_path);
                    FavoritesData { stations: Vec::new() }
                }
            }
        } else {
            info!("Creating new favorites file");
            FavoritesData {
                stations: Vec::new(),
            }
        };

        Ok(Self {
            file_path,
            favorites,
        })
    }

    pub fn add(&mut self, station: &Station) -> Result<()> {
        // Check if already in favorites
        if self.is_favorite(&station.station_uuid) {
            return Ok(());
        }

        info!("Adding station to favorites: {}", station.name);
        self.favorites.stations.push(FavoriteStation::from(station));
        self.save()
    }

    pub fn remove(&mut self, uuid: &str) -> Result<()> {
        info!("Removing station from favorites: {}", uuid);
        self.favorites.stations.retain(|s| s.uuid != uuid);
        self.save()
    }

    pub fn is_favorite(&self, uuid: &str) -> bool {
        self.favorites.stations.iter().any(|s| s.uuid == uuid)
    }

    pub fn get_all(&self) -> &[FavoriteStation] {
        &self.favorites.stations
    }

    fn save(&self) -> Result<()> {
        debug!("Saving favorites to: {:?}", self.file_path);
        let contents = toml::to_string_pretty(&self.favorites)
            .context("Failed to serialize favorites")?;
        fs::write(&self.file_path, contents)
            .context("Failed to write favorites file")?;
        Ok(())
    }
}
