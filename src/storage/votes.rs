use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

const VOTE_TTL_HOURS: i64 = 24;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteRecord {
    pub station_uuid: String,
    pub voted_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct VotesData {
    votes: Vec<VoteRecord>,
}

pub struct VoteManager {
    file_path: PathBuf,
    data: VotesData,
}

impl VoteManager {
    pub fn new(data_dir: &PathBuf) -> Result<Self> {
        let file_path = data_dir.join("votes.toml");

        let mut data: VotesData = if file_path.exists() {
            debug!("Loading votes from: {:?}", file_path);
            let contents = fs::read_to_string(&file_path)
                .context("Failed to read votes file")?;
            toml::from_str(&contents).context("Failed to parse votes file")?
        } else {
            VotesData::default()
        };

        // Drop expired entries on load
        let cutoff = Utc::now() - Duration::hours(VOTE_TTL_HOURS);
        data.votes.retain(|v| v.voted_at > cutoff);

        Ok(Self { file_path, data })
    }

    /// Returns true if the station was voted for within the last 24 hours.
    pub fn has_voted_recently(&self, station_uuid: &str) -> bool {
        let cutoff = Utc::now() - Duration::hours(VOTE_TTL_HOURS);
        self.data
            .votes
            .iter()
            .any(|v| v.station_uuid == station_uuid && v.voted_at > cutoff)
    }

    /// Record a vote for the given station and persist it.
    pub fn record_vote(&mut self, station_uuid: &str) -> Result<()> {
        // Remove any existing (expired) entry for this station first
        self.data.votes.retain(|v| v.station_uuid != station_uuid);
        self.data.votes.push(VoteRecord {
            station_uuid: station_uuid.to_string(),
            voted_at: Utc::now(),
        });
        info!("Recorded vote for station: {}", station_uuid);
        self.save()
    }

    /// Remove votes older than 24 hours and persist.
    pub fn cleanup_expired(&mut self) -> Result<()> {
        let cutoff = Utc::now() - Duration::hours(VOTE_TTL_HOURS);
        let before = self.data.votes.len();
        self.data.votes.retain(|v| v.voted_at > cutoff);
        let removed = before - self.data.votes.len();
        if removed > 0 {
            info!("Cleaned up {} expired vote(s)", removed);
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        debug!("Saving votes to: {:?}", self.file_path);
        let contents = toml::to_string_pretty(&self.data)
            .context("Failed to serialize votes")?;
        fs::write(&self.file_path, contents)
            .context("Failed to write votes file")?;
        Ok(())
    }
}
