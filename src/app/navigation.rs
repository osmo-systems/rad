use anyhow::Result;
use tui_kit::LogLevel;
use rad::Station;

use super::{App, BrowseMode, FocusedWidget, Tab};

impl App {
    pub fn next_tab(&mut self) {
        if matches!(self.current_tab, Tab::Browse) {
            self.browse_stations = self.stations.clone();
        }
        let has_autovote = self.config.autovote_enabled;
        self.current_tab = match self.current_tab {
            Tab::Browse => Tab::Favorites,
            Tab::Favorites => Tab::History,
            Tab::History => if has_autovote { Tab::Autovote } else { Tab::Browse },
            Tab::Autovote => Tab::Browse,
        };
        self.focused_widget = FocusedWidget::StationList;
        self.autovote_selected = 0;
        self.reload_current_tab();
    }

    pub fn prev_tab(&mut self) {
        if matches!(self.current_tab, Tab::Browse) {
            self.browse_stations = self.stations.clone();
        }
        let has_autovote = self.config.autovote_enabled;
        self.current_tab = match self.current_tab {
            Tab::Browse => if has_autovote { Tab::Autovote } else { Tab::History },
            Tab::Favorites => Tab::Browse,
            Tab::History => Tab::Favorites,
            Tab::Autovote => Tab::History,
        };
        self.focused_widget = FocusedWidget::StationList;
        self.autovote_selected = 0;
        self.reload_current_tab();
    }

    pub fn select_next(&mut self) {
        if self.browse_list_mode {
            let max = match self.browse_mode {
                BrowseMode::ByCountry => self.countries.len(),
                BrowseMode::ByGenre => self.genres.len(),
                BrowseMode::ByLanguage => self.languages.len(),
                _ => 0,
            };
            if max > 0 && self.browse_list_index < max - 1 {
                self.browse_list_index += 1;
            }
        } else if !self.stations.is_empty() && self.selected_index < self.stations.len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn select_prev(&mut self) {
        if self.browse_list_mode {
            if self.browse_list_index > 0 {
                self.browse_list_index -= 1;
            }
        } else if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn page_down(&mut self) {
        let jump_size = self.visible_stations_count.max(1);

        if self.browse_list_mode {
            let max = match self.browse_mode {
                BrowseMode::ByCountry => self.countries.len(),
                BrowseMode::ByGenre => self.genres.len(),
                BrowseMode::ByLanguage => self.languages.len(),
                _ => 0,
            };
            if max > 0 {
                self.browse_list_index = (self.browse_list_index + jump_size).min(max - 1);
            }
        } else if !self.stations.is_empty() {
            self.selected_index = (self.selected_index + jump_size).min(self.stations.len() - 1);
        }
    }

    pub fn page_up(&mut self) {
        let jump_size = self.visible_stations_count.max(1);

        if self.browse_list_mode {
            self.browse_list_index = self.browse_list_index.saturating_sub(jump_size);
        } else {
            self.selected_index = self.selected_index.saturating_sub(jump_size);
        }
    }

    pub fn get_selected_station(&self) -> Option<&Station> {
        self.stations.get(self.selected_index)
    }

    #[allow(dead_code)]
    pub fn set_browse_mode(&mut self, mode: BrowseMode) {
        self.browse_mode = mode;
        self.browse_list_mode = matches!(mode, BrowseMode::ByCountry | BrowseMode::ByGenre | BrowseMode::ByLanguage);
        self.browse_list_index = 0;
    }

    #[allow(dead_code)]
    pub async fn load_browse_lists(&mut self) -> Result<()> {
        self.loading = true;

        if self.countries.is_empty() {
            match self.api_client.get_countries().await {
                Ok(countries) => {
                    self.countries = countries.iter()
                        .filter(|c| c.station_count > 0)
                        .map(|c| c.name.clone())
                        .collect();
                }
                Err(e) => {
                    tracing::warn!("Failed to load countries: {}", e);
                }
            }
        }

        if self.genres.is_empty() {
            match self.api_client.get_tags(200).await {
                Ok(tags) => {
                    self.genres = tags.iter()
                        .filter(|t| t.station_count > 0)
                        .map(|t| t.name.clone())
                        .collect();
                }
                Err(e) => {
                    tracing::warn!("Failed to load tags: {}", e);
                }
            }
        }

        if self.languages.is_empty() {
            match self.api_client.get_languages().await {
                Ok(languages) => {
                    self.languages = languages.iter()
                        .filter(|l| l.station_count > 0)
                        .map(|l| l.name.clone())
                        .collect();
                }
                Err(e) => {
                    tracing::warn!("Failed to load languages: {}", e);
                }
            }
        }

        self.loading = false;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn select_from_browse_list(&mut self) -> Result<()> {
        if !self.browse_list_mode {
            return Ok(());
        }

        self.loading = true;
        self.browse_list_mode = false;

        let result: Result<Vec<Station>> = match self.browse_mode {
            BrowseMode::ByCountry => {
                if let Some(country) = self.countries.get(self.browse_list_index) {
                    let cache_key = format!("country_{}", country);
                    if let Some(cached_stations) = self.cache.get(&cache_key) {
                        Ok(cached_stations)
                    } else {
                        let stations = self.api_client.get_stations_by_country(country, self.config.station_limit).await?;
                        self.cache.set(&cache_key, stations.clone())?;
                        Ok(stations)
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            BrowseMode::ByGenre => {
                if let Some(genre) = self.genres.get(self.browse_list_index) {
                    let cache_key = format!("genre_{}", genre);
                    if let Some(cached_stations) = self.cache.get(&cache_key) {
                        Ok(cached_stations)
                    } else {
                        let stations = self.api_client.get_stations_by_tag(genre, self.config.station_limit).await?;
                        self.cache.set(&cache_key, stations.clone())?;
                        Ok(stations)
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            BrowseMode::ByLanguage => {
                if let Some(language) = self.languages.get(self.browse_list_index) {
                    let cache_key = format!("language_{}", language);
                    if let Some(cached_stations) = self.cache.get(&cache_key) {
                        Ok(cached_stations)
                    } else {
                        let stations = self.api_client.get_stations_by_language(language, self.config.station_limit).await?;
                        self.cache.set(&cache_key, stations.clone())?;
                        Ok(stations)
                    }
                } else {
                    Ok(Vec::new())
                }
            }
            _ => Ok(Vec::new()),
        };

        match result {
            Ok(stations) => {
                self.stations = stations;
                self.browse_stations = self.stations.clone();
                self.selected_index = 0;
                self.status_message = Some(format!("Loaded {} stations", self.stations.len()));
            }
            Err(e) => {
                self.status_message = Some(format!("Failed to load stations: {}", e));
            }
        }

        self.loading = false;
        Ok(())
    }

    #[allow(dead_code)]
    pub async fn load_popular_stations(&mut self) -> Result<()> {
        self.loading = true;
        tracing::info!("Loading popular stations...");
        self.add_log(LogLevel::Debug, "Loading popular stations...".to_string());

        let cache_key = "popular";
        let stations = if let Some(cached) = self.cache.get(cache_key) {
            tracing::info!("Using cached popular stations: {} stations", cached.len());
            self.add_log(LogLevel::Debug, format!("Loaded {} stations from cache", cached.len()));
            cached
        } else {
            tracing::info!("Fetching popular stations from API...");
            self.add_log(LogLevel::Debug, "Fetching stations from API...".to_string());
            match self.api_client.get_popular_stations(self.config.station_limit).await {
                Ok(stations) => {
                    tracing::info!("Fetched {} popular stations from API", stations.len());
                    self.add_log(LogLevel::Info, format!("Fetched {} stations from API", stations.len()));
                    if let Err(e) = self.cache.set(cache_key, stations.clone()) {
                        tracing::warn!("Failed to cache stations: {}", e);
                    }
                    stations
                }
                Err(e) => {
                    tracing::error!("Failed to load popular stations: {}", e);
                    self.loading = false;
                    let msg = format!("Failed to load stations: {}", e);
                    self.status_message = Some(msg.clone());
                    self.add_log(LogLevel::Error, msg);
                    return Err(e);
                }
            }
        };

        self.stations = stations;
        self.browse_stations = self.stations.clone();
        self.selected_index = 0;
        self.loading = false;
        let msg = format!("Loaded {} popular stations", self.stations.len());
        self.status_message = Some(msg.clone());
        self.add_log(LogLevel::Info, msg);
        tracing::info!("Popular stations loaded successfully: {} stations", self.stations.len());

        Ok(())
    }

    pub fn reload_current_tab(&mut self) {
        match self.current_tab {
            Tab::Autovote => {
                self.stations = Vec::new();
                self.selected_index = 0;
            }
            Tab::Browse => {
                self.stations = self.browse_stations.clone();
            }
            Tab::Favorites => {
                self.stations = self.favorites.get_all()
                    .iter()
                    .map(|f| Station {
                        station_uuid: f.uuid.clone(),
                        change_uuid: String::new(),
                        name: f.name.clone(),
                        url: f.url.clone(),
                        url_resolved: f.url.clone(),
                        homepage: String::new(),
                        favicon: String::new(),
                        tags: f.tags.clone(),
                        country: f.country.clone(),
                        country_code: String::new(),
                        state: String::new(),
                        language: String::new(),
                        language_codes: String::new(),
                        votes: 0,
                        codec: f.codec.clone(),
                        bitrate: f.bitrate,
                        hls: 0,
                        last_check_ok: 1,
                        last_check_time: String::new(),
                        last_check_ok_time: String::new(),
                        click_timestamp: String::new(),
                        click_count: 0,
                        click_trend: 0,
                    })
                    .collect();
                self.selected_index = self.selected_index.min(self.stations.len().saturating_sub(1));
            }
            Tab::History => {
                self.stations = self.history.get_all()
                    .iter()
                    .map(|h| Station {
                        station_uuid: h.uuid.clone(),
                        change_uuid: String::new(),
                        name: h.name.clone(),
                        url: h.url.clone(),
                        url_resolved: h.url.clone(),
                        homepage: String::new(),
                        favicon: String::new(),
                        tags: h.tags.clone(),
                        country: h.country.clone(),
                        country_code: String::new(),
                        state: String::new(),
                        language: String::new(),
                        language_codes: String::new(),
                        votes: 0,
                        codec: h.codec.clone(),
                        bitrate: h.bitrate,
                        hls: 0,
                        last_check_ok: 1,
                        last_check_time: String::new(),
                        last_check_ok_time: String::new(),
                        click_timestamp: String::new(),
                        click_count: 0,
                        click_trend: 0,
                    })
                    .collect();
                self.selected_index = 0;
            }
        }
    }
}
