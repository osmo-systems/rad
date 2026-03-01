use anyhow::Result;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::api::{RadioBrowserClient, Station};
use crate::config::Config;
use crate::player::{PlayerCommand, PlayerInfo};
use crate::storage::{CacheManager, FavoritesManager, HistoryManager};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Browse,
    Favorites,
    History,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BrowseMode {
    Popular,
    Search,
    ByCountry,
    ByGenre,
    ByLanguage,
}

pub struct App {
    pub running: bool,
    pub current_tab: Tab,
    pub browse_mode: BrowseMode,
    
    // Station lists
    pub stations: Vec<Station>,
    pub selected_index: usize,
    pub scroll_offset: usize,
    
    // Search/filter
    pub search_query: String,
    pub search_mode: bool,
    
    // Player
    pub player_info: PlayerInfo,
    pub player_cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
    
    // API & Storage
    pub api_client: RadioBrowserClient,
    pub favorites: FavoritesManager,
    pub history: HistoryManager,
    pub cache: CacheManager,
    pub config: Config,
    
    // Loading state
    pub loading: bool,
    pub status_message: Option<String>,
    
    // Error popup
    pub error_popup: Option<String>,
    
    // Help popup
    pub help_popup: bool,
    
    // Lists for browse modes
    pub countries: Vec<String>,
    pub genres: Vec<String>,
    pub languages: Vec<String>,
    pub browse_list_index: usize,
    pub browse_list_mode: bool,
    
    // Visualizer
    pub visualizer_data: Vec<f32>,
}

impl App {
    pub async fn new(
        data_dir: PathBuf,
        api_client: RadioBrowserClient,
    ) -> Result<Self> {
        let config = Config::load(&data_dir)?;
        let favorites = FavoritesManager::new(&data_dir)?;
        let history = HistoryManager::new(&data_dir, config.max_history_entries)?;
        let cache = CacheManager::new(&data_dir, config.cache_duration_secs)?;
        
        let (player_cmd_tx, _player_cmd_rx) = mpsc::unbounded_channel();
        let player_info = PlayerInfo {
            state: crate::player::PlayerState::Stopped,
            station_name: String::new(),
            station_url: String::new(),
            volume: config.default_volume,
            error_message: None,
        };

        let app = Self {
            running: true,
            current_tab: Tab::Browse,
            browse_mode: BrowseMode::Popular,
            
            stations: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            
            search_query: String::new(),
            search_mode: false,
            
            player_info,
            player_cmd_tx,
            
            api_client,
            favorites,
            history,
            cache,
            config,
            
            loading: false,
            status_message: None,
            
            error_popup: None,
            help_popup: false,
            
            countries: Vec::new(),
            genres: Vec::new(),
            languages: Vec::new(),
            browse_list_index: 0,
            browse_list_mode: false,
            
            visualizer_data: vec![0.0; 50],
        };

        Ok(app)
    }
    
    pub fn show_error(&mut self, message: String) {
        self.error_popup = Some(message);
    }
    
    pub fn close_error_popup(&mut self) {
        self.error_popup = None;
        // Also clear player error if it exists
        if self.player_info.error_message.is_some() {
            self.player_info.error_message = None;
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            Tab::Browse => Tab::Favorites,
            Tab::Favorites => Tab::History,
            Tab::History => Tab::Browse,
        };
        self.reload_current_tab();
    }

    pub fn prev_tab(&mut self) {
        self.current_tab = match self.current_tab {
            Tab::Browse => Tab::History,
            Tab::Favorites => Tab::Browse,
            Tab::History => Tab::Favorites,
        };
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
        if self.browse_list_mode {
            let max = match self.browse_mode {
                BrowseMode::ByCountry => self.countries.len(),
                BrowseMode::ByGenre => self.genres.len(),
                BrowseMode::ByLanguage => self.languages.len(),
                _ => 0,
            };
            if max > 0 {
                self.browse_list_index = (self.browse_list_index + 10).min(max - 1);
            }
        } else if !self.stations.is_empty() {
            self.selected_index = (self.selected_index + 10).min(self.stations.len() - 1);
        }
    }

    pub fn page_up(&mut self) {
        if self.browse_list_mode {
            self.browse_list_index = self.browse_list_index.saturating_sub(10);
        } else {
            self.selected_index = self.selected_index.saturating_sub(10);
        }
    }

    pub fn get_selected_station(&self) -> Option<&Station> {
        self.stations.get(self.selected_index)
    }

    pub async fn play_selected(&mut self) -> Result<()> {
        tracing::info!("play_selected called, stations count: {}, selected_index: {}", self.stations.len(), self.selected_index);
        
        if self.stations.is_empty() {
            tracing::warn!("No stations available to play");
            self.status_message = Some("No stations loaded. Try loading popular stations first.".to_string());
            return Ok(());
        }
        
        if let Some(station) = self.get_selected_station() {
            let station = station.clone();
            tracing::info!("Playing station: {} - URL: {}", station.name, station.url_resolved);
            
            // Count click on API in background (non-blocking with timeout)
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

            // Add to history
            tracing::info!("Adding to history");
            if let Err(e) = self.history.add(&station) {
                tracing::warn!("Failed to add to history: {}", e);
            }

            // Send play command
            tracing::info!("Sending play command");
            match self.player_cmd_tx.send(PlayerCommand::Play(
                station.name.clone(),
                station.url_resolved.clone(),
            )) {
                Ok(_) => {
                    self.status_message = Some(format!("Playing: {}", station.name));
                    tracing::info!("Play command sent successfully to player");
                }
                Err(e) => {
                    tracing::error!("Failed to send play command: {}", e);
                    self.status_message = Some(format!("Failed to send play command: {}", e));
                    return Err(e.into());
                }
            }
        } else {
            tracing::warn!("get_selected_station returned None");
            self.status_message = Some("No station selected".to_string());
        }
        Ok(())
    }

    pub fn pause(&mut self) -> Result<()> {
        self.player_cmd_tx.send(PlayerCommand::Pause)?;
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        self.player_cmd_tx.send(PlayerCommand::Resume)?;
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        self.player_cmd_tx.send(PlayerCommand::Stop)?;
        Ok(())
    }

    pub fn reload(&mut self) -> Result<()> {
        self.player_cmd_tx.send(PlayerCommand::Reload)?;
        self.status_message = Some("Reloading station...".to_string());
        Ok(())
    }

    pub fn volume_up(&mut self) -> Result<()> {
        let new_volume = (self.player_info.volume + 0.05).min(1.0);
        self.player_cmd_tx.send(PlayerCommand::SetVolume(new_volume))?;
        self.player_info.volume = new_volume;
        Ok(())
    }

    pub fn volume_down(&mut self) -> Result<()> {
        let new_volume = (self.player_info.volume - 0.05).max(0.0);
        self.player_cmd_tx.send(PlayerCommand::SetVolume(new_volume))?;
        self.player_info.volume = new_volume;
        Ok(())
    }

    pub async fn toggle_favorite(&mut self) -> Result<()> {
        if let Some(station) = self.get_selected_station() {
            let uuid = station.station_uuid.clone();
            let station_clone = station.clone();
            if self.favorites.is_favorite(&uuid) {
                self.favorites.remove(&uuid)?;
                self.status_message = Some("Removed from favorites".to_string());
            } else {
                self.favorites.add(&station_clone)?;
                self.status_message = Some("Added to favorites".to_string());
            }
            
            // Reload if we're on favorites tab
            if self.current_tab == Tab::Favorites {
                self.reload_current_tab();
            }
        }
        Ok(())
    }

    pub async fn vote_for_selected(&mut self) -> Result<()> {
        if let Some(station) = self.get_selected_station() {
            let uuid = station.station_uuid.clone();
            let name = station.name.clone();
            match self.api_client.vote_for_station(&uuid).await {
                Ok(_) => {
                    self.status_message = Some(format!("Voted for: {}", name));
                }
                Err(e) => {
                    self.status_message = Some(format!("Failed to vote: {}", e));
                }
            }
        }
        Ok(())
    }

    pub fn toggle_search_mode(&mut self) {
        self.search_mode = !self.search_mode;
        if self.search_mode {
            self.search_query.clear();
        }
    }

    pub fn add_search_char(&mut self, c: char) {
        if self.search_mode {
            self.search_query.push(c);
        }
    }

    pub fn remove_search_char(&mut self) {
        if self.search_mode {
            self.search_query.pop();
        }
    }

    pub async fn perform_search(&mut self) -> Result<()> {
        if !self.search_query.is_empty() {
            self.loading = true;
            self.browse_mode = BrowseMode::Search;
            self.browse_list_mode = false;
            
            match self.api_client.search_stations(&self.search_query, self.config.station_limit).await {
                Ok(stations) => {
                    self.stations = stations;
                    self.selected_index = 0;
                    self.search_mode = false;
                    self.status_message = Some(format!("Found {} stations", self.stations.len()));
                }
                Err(e) => {
                    self.status_message = Some(format!("Search failed: {}", e));
                }
            }
            
            self.loading = false;
        }
        Ok(())
    }

    pub fn set_browse_mode(&mut self, mode: BrowseMode) {
        self.browse_mode = mode;
        self.browse_list_mode = matches!(mode, BrowseMode::ByCountry | BrowseMode::ByGenre | BrowseMode::ByLanguage);
        self.browse_list_index = 0;
    }

    pub async fn load_browse_lists(&mut self) -> Result<()> {
        self.loading = true;
        
        // Load countries
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

        // Load genres
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

        // Load languages
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

    pub async fn select_from_browse_list(&mut self) -> Result<()> {
        if !self.browse_list_mode {
            return Ok(());
        }

        self.loading = true;
        self.browse_list_mode = false;

        let result: Result<Vec<Station>> = match self.browse_mode {
            BrowseMode::ByCountry => {
                if let Some(country) = self.countries.get(self.browse_list_index) {
                    // Check cache first
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

    pub async fn load_popular_stations(&mut self) -> Result<()> {
        self.loading = true;
        tracing::info!("Loading popular stations...");
        
        let cache_key = "popular";
        let stations = if let Some(cached) = self.cache.get(cache_key) {
            tracing::info!("Using cached popular stations: {} stations", cached.len());
            cached
        } else {
            tracing::info!("Fetching popular stations from API...");
            match self.api_client.get_popular_stations(self.config.station_limit).await {
                Ok(stations) => {
                    tracing::info!("Fetched {} popular stations from API", stations.len());
                    if let Err(e) = self.cache.set(cache_key, stations.clone()) {
                        tracing::warn!("Failed to cache stations: {}", e);
                    }
                    stations
                }
                Err(e) => {
                    tracing::error!("Failed to load popular stations: {}", e);
                    self.loading = false;
                    self.status_message = Some(format!("Failed to load stations: {}", e));
                    return Err(e);
                }
            }
        };

        self.stations = stations;
        self.selected_index = 0;
        self.loading = false;
        self.status_message = Some(format!("Loaded {} popular stations", self.stations.len()));
        tracing::info!("Popular stations loaded successfully: {} stations", self.stations.len());
        
        Ok(())
    }

    pub fn reload_current_tab(&mut self) {
        match self.current_tab {
            Tab::Browse => {
                // Keep current browse state
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
                        codec: String::new(),
                        bitrate: 0,
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
                        codec: String::new(),
                        bitrate: 0,
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

    pub fn update_visualizer(&mut self) {
        // Simple random visualizer for now
        // In a real implementation, this would analyze audio data
        use rand::Rng;
        let mut rng = rand::thread_rng();
        
        if self.player_info.state == crate::player::PlayerState::Playing {
            for i in 0..self.visualizer_data.len() {
                let target = rng.gen_range(0.0..1.0);
                self.visualizer_data[i] = self.visualizer_data[i] * 0.7 + target * 0.3;
            }
        } else {
            for i in 0..self.visualizer_data.len() {
                self.visualizer_data[i] *= 0.9;
            }
        }
    }
}
