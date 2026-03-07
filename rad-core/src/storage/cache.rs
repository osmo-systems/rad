use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tracing::{debug, info};

use crate::api::Station;

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedStations {
    pub stations: Vec<Station>,
    pub cached_at: SystemTime,
    pub cache_key: String,
}

pub struct CacheManager {
    cache_dir: PathBuf,
    cache_duration: Duration,
}

impl CacheManager {
    pub fn new(data_dir: &PathBuf, cache_duration_secs: u64) -> Result<Self> {
        let cache_dir = data_dir.join("cache");
        
        if !cache_dir.exists() {
            fs::create_dir_all(&cache_dir)
                .context("Failed to create cache directory")?;
        }

        Ok(Self {
            cache_dir,
            cache_duration: Duration::from_secs(cache_duration_secs),
        })
    }

    pub fn get(&self, cache_key: &str) -> Option<Vec<Station>> {
        let cache_file = self.cache_dir.join(format!("{}.json", cache_key));
        
        if !cache_file.exists() {
            return None;
        }

        match fs::read_to_string(&cache_file) {
            Ok(contents) => {
                match serde_json::from_str::<CachedStations>(&contents) {
                    Ok(cached) => {
                        // Check if cache is still valid
                        if let Ok(elapsed) = cached.cached_at.elapsed() {
                            if elapsed < self.cache_duration {
                                debug!("Cache hit for key: {}", cache_key);
                                return Some(cached.stations);
                            } else {
                                debug!("Cache expired for key: {}", cache_key);
                            }
                        }
                    }
                    Err(e) => {
                        debug!("Failed to parse cache file: {}", e);
                    }
                }
            }
            Err(e) => {
                debug!("Failed to read cache file: {}", e);
            }
        }

        None
    }

    pub fn set(&self, cache_key: &str, stations: Vec<Station>) -> Result<()> {
        let cache_file = self.cache_dir.join(format!("{}.json", cache_key));
        
        let cached = CachedStations {
            stations,
            cached_at: SystemTime::now(),
            cache_key: cache_key.to_string(),
        };

        let contents = serde_json::to_string(&cached)
            .context("Failed to serialize cache")?;
        
        fs::write(&cache_file, contents)
            .context("Failed to write cache file")?;

        debug!("Cached {} stations for key: {}", cached.stations.len(), cache_key);
        
        Ok(())
    }

    pub fn clear(&self) -> Result<()> {
        info!("Clearing cache");
        
        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
                    fs::remove_file(entry.path())?;
                }
            }
        }

        Ok(())
    }
}
