use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHistoryEntry {
    pub query: String,
    pub timestamp: DateTime<Utc>,
    pub result_count: Option<usize>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchHistoryData {
    pub queries: Vec<SearchHistoryEntry>,
}

pub struct SearchHistoryManager {
    file_path: PathBuf,
    history: SearchHistoryData,
    max_entries: usize,
}

impl SearchHistoryManager {
    pub fn new(data_dir: &PathBuf) -> Result<Self> {
        let file_path = data_dir.join("search_history.toml");
        
        let history = if file_path.exists() {
            debug!("Loading search history from: {:?}", file_path);
            let contents = fs::read_to_string(&file_path)
                .context("Failed to read search history file")?;
            toml::from_str(&contents)
                .context("Failed to parse search history file")?
        } else {
            info!("Creating new search history file");
            SearchHistoryData {
                queries: Vec::new(),
            }
        };

        Ok(Self {
            file_path,
            history,
            max_entries: 50,
        })
    }

    pub fn add_query(&mut self, query: String, result_count: Option<usize>) -> Result<()> {
        // Remove existing entry with same query if it exists
        self.history.queries.retain(|entry| entry.query != query);

        // Add new entry at the front
        let entry = SearchHistoryEntry {
            query,
            timestamp: Utc::now(),
            result_count,
        };
        self.history.queries.insert(0, entry);

        // Keep only max_entries
        if self.history.queries.len() > self.max_entries {
            self.history.queries.truncate(self.max_entries);
        }

        self.save()
    }

    pub fn get_recent_queries(&self, limit: usize) -> Vec<&SearchHistoryEntry> {
        self.history.queries.iter().take(limit).collect()
    }

    pub fn get_all(&self) -> &[SearchHistoryEntry] {
        &self.history.queries
    }

    fn save(&self) -> Result<()> {
        let contents = toml::to_string_pretty(&self.history)
            .context("Failed to serialize search history")?;
        fs::write(&self.file_path, contents)
            .context("Failed to write search history file")?;
        Ok(())
    }
}
