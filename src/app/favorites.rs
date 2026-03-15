use anyhow::Result;
use tui_kit::{LogLevel, ToastLevel};
use rad::Station;

use super::{App, Tab};

impl App {
    pub async fn toggle_favorite(&mut self) -> Result<()> {
        if let Some(station) = self.get_selected_station() {
            let uuid = station.station_uuid.clone();
            let name = station.name.clone();
            let station_clone = station.clone();
            if self.favorites.is_favorite(&uuid) {
                self.favorites.remove(&uuid)?;
                self.status_message = Some("Removed from favorites".to_string());
                self.show_toast(format!("Removed {} from favorites", name), ToastLevel::Warning);
            } else {
                self.favorites.add(&station_clone)?;
                self.status_message = Some("Added to favorites".to_string());
                self.show_toast(format!("Added {} to favorites", name), ToastLevel::Success);
            }

            if self.current_tab == Tab::Favorites {
                self.reload_current_tab();
            }
        }
        Ok(())
    }

    pub async fn vote_for_selected(&mut self) -> Result<()> {
        let (uuid, name) = match self.get_selected_station() {
            Some(s) => (s.station_uuid.clone(), s.name.clone()),
            None => {
                self.add_log(LogLevel::Warning, "Vote: no station selected".to_string());
                return Ok(());
            }
        };

        if self.vote_manager.has_voted_recently(&uuid) {
            self.add_log(LogLevel::Warning, format!("Already voted for {} (24h cooldown)", name));
            self.show_toast(format!("Already voted for {}", name), ToastLevel::Warning);
            return Ok(());
        }

        self.add_log(LogLevel::Debug, format!("Voting for {}...", name));
        let _ = self.vote_manager.record_vote(&uuid);

        match self.api_client.vote_for_station(&uuid).await {
            Ok(response) if response.ok => {
                self.add_log(LogLevel::Info, format!("Vote cast for {}", name));
                self.show_toast(format!("Voted for {}", name), ToastLevel::Success);
            }
            Ok(response) => {
                self.add_log(LogLevel::Warning, format!("Vote rejected: {}", response.message));
                self.show_toast(format!("Vote: {}", response.message), ToastLevel::Warning);
            }
            Err(e) => {
                self.add_log(LogLevel::Error, format!("Vote API error: {}", e));
                self.show_toast("Vote saved locally (API error)".to_string(), ToastLevel::Warning);
            }
        }

        Ok(())
    }

    /// Vote for all favorite stations that haven't been voted for in the last 24h.
    pub async fn auto_vote_favorites(&mut self) -> Result<()> {
        if !self.config.auto_vote_favorites {
            return Ok(());
        }

        let _ = self.vote_manager.cleanup_expired();

        let uuids: Vec<(String, String)> = self
            .favorites
            .get_all()
            .iter()
            .map(|f| (f.uuid.clone(), f.name.clone()))
            .collect();

        for (uuid, name) in uuids {
            if !self.vote_manager.has_voted_recently(&uuid) {
                match self.api_client.vote_for_station(&uuid).await {
                    Ok(_) => {
                        let _ = self.vote_manager.record_vote(&uuid);
                        self.add_log(LogLevel::Info, format!("Auto-voted for favorite: {}", name));
                    }
                    Err(e) => {
                        tracing::warn!("Auto-vote failed for {}: {}", name, e);
                    }
                }
            }
        }
        Ok(())
    }

    pub fn toggle_autovote(&mut self) {
        let station = if self.current_tab == Tab::Autovote {
            self.autovote.get_all().get(self.autovote_selected).map(|s| Station {
                station_uuid: s.uuid.clone(),
                change_uuid: String::new(),
                name: s.name.clone(),
                url: s.url.clone(),
                url_resolved: s.url.clone(),
                homepage: String::new(),
                favicon: String::new(),
                tags: String::new(),
                country: s.country.clone(),
                country_code: String::new(),
                state: String::new(),
                language: String::new(),
                language_codes: String::new(),
                votes: 0,
                codec: s.codec.clone(),
                bitrate: s.bitrate,
                hls: 0,
                last_check_ok: 1,
                last_check_time: String::new(),
                last_check_ok_time: String::new(),
                click_timestamp: String::new(),
                click_count: 0,
                click_trend: 0,
            })
        } else {
            self.get_selected_station().cloned()
        };

        if let Some(s) = station {
            let uuid = s.station_uuid.clone();
            let name = s.name.clone();
            if self.autovote.contains(&uuid) {
                if let Err(e) = self.autovote.remove(&uuid) {
                    tracing::error!("Failed to remove from autovote: {}", e);
                } else {
                    self.show_toast(format!("Removed {} from autovote", name), ToastLevel::Warning);
                    let count = self.autovote.get_all().len();
                    if count == 0 && self.current_tab == Tab::Autovote {
                        if matches!(self.current_tab, Tab::Browse) {
                            self.browse_stations = self.stations.clone();
                        }
                        self.current_tab = Tab::Favorites;
                        self.reload_current_tab();
                        self.autovote_selected = 0;
                    } else if self.autovote_selected >= count && count > 0 {
                        self.autovote_selected = count - 1;
                    }
                }
            } else {
                if let Err(e) = self.autovote.add(&s) {
                    tracing::error!("Failed to add to autovote: {}", e);
                } else {
                    self.show_toast(format!("Added {} to autovote", name), ToastLevel::Success);
                }
            }
        }
    }

    /// Vote for all stations in the autovote list that haven't been voted for in the last 24h.
    pub async fn auto_vote_autovote_list(&mut self) -> Result<()> {
        let _ = self.vote_manager.cleanup_expired();

        let entries: Vec<(String, String)> = self.autovote.get_all()
            .iter()
            .map(|s| (s.uuid.clone(), s.name.clone()))
            .collect();

        for (uuid, name) in entries {
            if !self.vote_manager.has_voted_recently(&uuid) {
                match self.api_client.vote_for_station(&uuid).await {
                    Ok(_) => {
                        let _ = self.vote_manager.record_vote(&uuid);
                        self.add_log(LogLevel::Info, format!("Auto-voted for: {}", name));
                    }
                    Err(e) => {
                        tracing::warn!("Auto-vote failed for {}: {}", name, e);
                    }
                }
            }
        }
        Ok(())
    }
}
