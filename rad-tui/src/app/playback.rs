use anyhow::Result;
use tui_kit::LogLevel;
use rad_core::PlayerDaemonConnection;

use super::App;

impl App {
    pub async fn play_selected(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        tracing::info!("play_selected called, stations count: {}, selected_index: {}", self.stations.len(), self.selected_index);

        if self.stations.is_empty() {
            tracing::warn!("No stations available to play");
            self.status_message = Some("No stations loaded. Try loading popular stations first.".to_string());
            return Ok(());
        }

        if let Some(station) = self.get_selected_station() {
            let station = station.clone();
            tracing::info!("Playing station: {} - URL: {}", station.name, station.url_resolved);

            let mut api_client_clone = self.api_client.clone();
            let station_uuid = station.station_uuid.clone();
            tokio::spawn(async move {
                tracing::debug!("Starting count_click API call in background");
                let click_result = tokio::time::timeout(
                    std::time::Duration::from_secs(2),
                    api_client_clone.count_click(&station_uuid)
                ).await;
                match click_result {
                    Ok(Ok(_)) => tracing::debug!("Click counted successfully"),
                    Ok(Err(e)) => tracing::warn!("Failed to count click: {}", e),
                    Err(_) => tracing::warn!("count_click timed out after 2 seconds"),
                }
            });

            tracing::info!("Adding to history");
            if let Err(e) = self.history.add(&station) {
                tracing::warn!("Failed to add to history: {}", e);
            }

            tracing::info!("Sending play command to daemon");
            self.add_log(LogLevel::Info, format!("Playing: {}", station.name));
            match daemon_conn.play(station.name.clone(), station.url_resolved.clone()).await {
                Ok(_) => {
                    self.status_message = Some(format!("Playing: {}", station.name));
                    tracing::info!("Play command sent successfully to daemon");

                    self.config.update_session_state(
                        self.player_info.volume,
                        Some(station.name.clone()),
                        Some(station.url_resolved.clone()),
                    );
                    let _ = self.config.save(&self.data_dir);
                }
                Err(e) => {
                    tracing::error!("Failed to send play command: {}", e);
                    let msg = format!("Failed to send play command: {}", e);
                    self.status_message = Some(msg.clone());
                    self.add_log(LogLevel::Error, msg);
                    return Err(e);
                }
            }
        } else {
            tracing::warn!("get_selected_station returned None");
            self.status_message = Some("No station selected".to_string());
        }
        Ok(())
    }

    pub async fn pause(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        daemon_conn.pause().await?;
        Ok(())
    }

    pub async fn resume(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        daemon_conn.resume().await?;
        Ok(())
    }

    pub async fn play_restored(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        if !self.player_info.station_url.is_empty() {
            tracing::info!("Playing restored station: {}", self.player_info.station_name);
            daemon_conn.play(
                self.player_info.station_name.clone(),
                self.player_info.station_url.clone(),
            ).await?;
            self.add_log(LogLevel::Info, format!("Playing: {}", self.player_info.station_name));
            self.status_message = Some(format!("Playing: {}", self.player_info.station_name));
        } else {
            tracing::warn!("No restored station to play");
            self.status_message = Some("No station to play".to_string());
        }
        Ok(())
    }

    pub async fn stop(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        daemon_conn.stop().await?;
        Ok(())
    }

    pub async fn reload(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        daemon_conn.reload().await?;
        self.status_message = Some("Reloading station...".to_string());
        Ok(())
    }

    pub async fn volume_up(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        let new_volume = (self.player_info.volume + 0.05).min(1.0);
        daemon_conn.set_volume(new_volume).await?;
        self.player_info.volume = new_volume;

        self.config.update_session_state(
            new_volume,
            self.config.last_station_name.clone(),
            self.config.last_station_url.clone(),
        );
        let _ = self.config.save(&self.data_dir);

        Ok(())
    }

    pub async fn volume_down(&mut self, daemon_conn: &mut PlayerDaemonConnection) -> Result<()> {
        let new_volume = (self.player_info.volume - 0.05).max(0.0);
        daemon_conn.set_volume(new_volume).await?;
        self.player_info.volume = new_volume;

        self.config.update_session_state(
            new_volume,
            self.config.last_station_name.clone(),
            self.config.last_station_url.clone(),
        );
        let _ = self.config.save(&self.data_dir);

        Ok(())
    }
}
