use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::api::{RadioBrowserClient, Station};
use crate::config::Config;
use crate::player::{PlayerCommand, PlayerInfo};
use crate::search::{AutocompleteData, SearchQuery, is_default_query, format_query};
use crate::storage::{CacheManager, FavoritesManager, HistoryManager, SearchHistoryManager};
use crate::ui::SearchPopup;

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
    pub browse_stations: Vec<Station>, // Cache for Browse tab stations
    pub selected_index: usize,
    pub scroll_offset: usize,
    
    // Advanced search
    pub search_popup: Option<SearchPopup>,
    pub current_query: SearchQuery,
    pub autocomplete_data: AutocompleteData,
    
    // Pagination
    pub current_page: usize,
    pub pages_cache: HashMap<usize, Vec<Station>>,
    pub is_last_page: bool,
    
    // Player
    pub player_info: PlayerInfo,
    pub player_cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
    
    // API & Storage
    pub api_client: RadioBrowserClient,
    pub favorites: FavoritesManager,
    pub history: HistoryManager,
    pub cache: CacheManager,
    pub search_history: SearchHistoryManager,
    pub config: Config,
    pub data_dir: PathBuf,
    
    // Loading state
    pub loading: bool,
    pub status_message: Option<String>,
    pub pending_search: bool, // Flag to trigger search on next loop iteration
    
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
    
    // Animation
    pub animation_frame: usize,
    
    // Status log
    pub status_log: Vec<String>,
    pub status_log_scroll: usize,
}

impl App {
    pub async fn new(
        data_dir: PathBuf,
        mut api_client: RadioBrowserClient,
    ) -> Result<Self> {
        let config = Config::load(&data_dir)?;
        let favorites = FavoritesManager::new(&data_dir)?;
        let history = HistoryManager::new(&data_dir, config.max_history_entries)?;
        let cache = CacheManager::new(&data_dir, config.cache_duration_secs)?;
        let search_history = SearchHistoryManager::new(&data_dir)?;
        
        let (player_cmd_tx, _player_cmd_rx) = mpsc::unbounded_channel();
        
        // Restore session state from config
        let restored_volume = config.last_volume.unwrap_or(config.default_volume);
        let player_info = PlayerInfo {
            state: crate::player::PlayerState::Stopped,
            station_name: config.last_station_name.clone().unwrap_or_default(),
            station_url: config.last_station_url.clone().unwrap_or_default(),
            volume: restored_volume,
            error_message: None,
        };

        // Load autocomplete data
        let autocomplete_data = AutocompleteData::load(&mut api_client).await
            .unwrap_or_default(); // Use default if loading fails

        // Start with default query (popular stations)
        let current_query = SearchQuery::default();

        let app = Self {
            running: true,
            current_tab: Tab::Browse,
            browse_mode: BrowseMode::Popular,
            
            stations: Vec::new(),
            browse_stations: Vec::new(),
            selected_index: 0,
            scroll_offset: 0,
            
            search_popup: None,
            current_query,
            autocomplete_data,
            
            current_page: 1,
            pages_cache: HashMap::new(),
            is_last_page: false,
            
            player_info,
            player_cmd_tx,
            
            api_client,
            favorites,
            history,
            cache,
            search_history,
            config,
            data_dir,
            
            loading: false,
            pending_search: false,
            status_message: None,
            
            error_popup: None,
            help_popup: false,
            
            countries: Vec::new(),
            genres: Vec::new(),
            languages: Vec::new(),
            browse_list_index: 0,
            browse_list_mode: false,
            
            animation_frame: 0,
            
            status_log: Vec::new(),
            status_log_scroll: 0,
        };
        
        Ok(app)
    }
    
    pub fn show_error(&mut self, message: String) {
        self.error_popup = Some(message);
    }
    
    pub fn close_error_popup(&mut self) {
        self.error_popup = None;
    }

    // Search popup methods

    pub fn open_search_popup(&mut self) {
        use crate::search::get_suggestions;
        
        // Pre-fill with current query if not default
        let initial_query = if is_default_query(&self.current_query) {
            String::new()
        } else {
            format_query(&self.current_query)
        };
        
        let mut popup = SearchPopup::new(initial_query.clone());
        
        // Initialize autocomplete with field names (since we start at position 0 or end)
        let suggestions = get_suggestions(&initial_query, popup.cursor_position, &self.autocomplete_data);
        popup.update_autocomplete(suggestions);
        
        self.search_popup = Some(popup);
    }

    pub fn close_search_popup(&mut self) {
        self.search_popup = None;
    }

    pub async fn execute_search(&mut self) -> Result<()> {
        tracing::info!("execute_search: Starting with query: {:?}", self.current_query);
        
        // Use the already-set current_query
        let mut query = self.current_query.clone();
        query.reset_pagination();
        
        tracing::info!("execute_search: Pagination reset");
        
        // Clear cache and reset pagination
        self.pages_cache.clear();
        self.current_page = 1;
        self.is_last_page = false;
        
        tracing::info!("execute_search: Cache cleared, calling API");
        
        // Load first page
        self.loading = true;
        match self.api_client.advanced_search(&query).await {
            Ok(stations) => {
                tracing::info!("execute_search: API returned {} stations", stations.len());
                self.is_last_page = stations.len() < query.limit;
                
                // Cache the page
                self.pages_cache.insert(1, stations.clone());
                
                self.stations = stations;
                self.browse_stations = self.stations.clone(); // Update browse cache
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.loading = false;
                
                tracing::info!("execute_search: Stations loaded, updating UI");
                
                // Add to search history if not default query
                if !is_default_query(&query) {
                    let query_str = format_query(&query);
                    if let Err(e) = self.search_history.add_query(query_str, Some(self.stations.len())) {
                        tracing::warn!("Failed to save search history: {}", e);
                    }
                }
                
                let msg = format!("Found {} stations", self.stations.len());
                self.status_message = Some(msg.clone());
                self.add_log(msg);
                
                tracing::info!("execute_search: Log added");
                
                // Close popup if it was open
                if self.search_popup.is_some() {
                    self.close_search_popup();
                }
                
                tracing::info!("execute_search: Completed successfully");
                Ok(())
            }
            Err(e) => {
                tracing::error!("execute_search: API call failed: {}", e);
                self.loading = false;
                // Network error - show error popup
                self.show_error(format!("Search failed: {}", e));
                Err(e)
            }
        }
    }

    pub async fn load_page(&mut self, page: usize) -> Result<()> {
        if page == 0 {
            return Ok(());
        }
        
        // Check cache first
        if let Some(stations) = self.pages_cache.get(&page) {
            self.stations = stations.clone();
            self.current_page = page;
            self.selected_index = 0;
            self.scroll_offset = 0;
            return Ok(());
        }
        
        // Not in cache, fetch from API
        let mut query = self.current_query.clone();
        query.offset = (page - 1) * query.limit;
        
        self.loading = true;
        match self.api_client.advanced_search(&query).await {
            Ok(stations) => {
                self.is_last_page = stations.len() < query.limit;
                
                // Cache the page (with LRU eviction if needed)
                if self.pages_cache.len() >= 5 {
                    // Simple LRU: remove the page furthest from current
                    let current = self.current_page;
                    let to_remove = self.pages_cache.keys()
                        .map(|k| (*k, (*k as i32 - current as i32).abs()))
                        .max_by_key(|(_, dist)| *dist)
                        .map(|(k, _)| k);
                    
                    if let Some(key) = to_remove {
                        self.pages_cache.remove(&key);
                    }
                }
                
                self.pages_cache.insert(page, stations.clone());
                
                self.stations = stations;
                self.current_page = page;
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.loading = false;
                
                Ok(())
            }
            Err(e) => {
                self.loading = false;
                self.show_error(format!("Failed to load page: {}", e));
                Err(e)
            }
        }
    }

    pub async fn next_page(&mut self) -> Result<()> {
        if !self.is_last_page {
            self.load_page(self.current_page + 1).await
        } else {
            Ok(())
        }
    }

    pub async fn prev_page(&mut self) -> Result<()> {
        if self.current_page > 1 {
            self.load_page(self.current_page - 1).await
        } else {
            Ok(())
        }
    }

    pub async fn first_page(&mut self) -> Result<()> {
        if self.current_page != 1 {
            self.load_page(1).await
        } else {
            Ok(())
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
    
    pub fn add_log(&mut self, message: String) {
        // Add timestamp to log message
        let timestamp = chrono::Local::now().format("%H:%M:%S");
        let log_entry = format!("[{}] {}", timestamp, message);
        self.status_log.push(log_entry);
        
        // Keep only last 100 log entries
        if self.status_log.len() > 100 {
            self.status_log.remove(0);
        }
        
        // Auto-scroll to bottom
        if self.status_log.len() > 0 {
            self.status_log_scroll = self.status_log.len().saturating_sub(1);
        }
    }
    
    pub fn scroll_log_up(&mut self) {
        if self.status_log_scroll > 0 {
            self.status_log_scroll -= 1;
        }
    }
    
    pub fn scroll_log_down(&mut self) {
        if !self.status_log.is_empty() && self.status_log_scroll < self.status_log.len() - 1 {
            self.status_log_scroll += 1;
        }
    }

    pub fn next_tab(&mut self) {
        // Cache browse stations before switching away
        if matches!(self.current_tab, Tab::Browse) {
            self.browse_stations = self.stations.clone();
        }
        
        self.current_tab = match self.current_tab {
            Tab::Browse => Tab::Favorites,
            Tab::Favorites => Tab::History,
            Tab::History => Tab::Browse,
        };
        self.reload_current_tab();
    }

    pub fn prev_tab(&mut self) {
        // Cache browse stations before switching away
        if matches!(self.current_tab, Tab::Browse) {
            self.browse_stations = self.stations.clone();
        }
        
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
            self.add_log(format!("Playing: {}", station.name));
            match self.player_cmd_tx.send(PlayerCommand::Play(
                station.name.clone(),
                station.url_resolved.clone(),
            )) {
                Ok(_) => {
                    self.status_message = Some(format!("Playing: {}", station.name));
                    tracing::info!("Play command sent successfully to player");
                    
                    // Save station and volume to config
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
                    self.add_log(msg);
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
        
        // Save volume to config
        self.config.update_session_state(
            new_volume,
            self.config.last_station_name.clone(),
            self.config.last_station_url.clone(),
        );
        let _ = self.config.save(&self.data_dir);
        
        Ok(())
    }

    pub fn volume_down(&mut self) -> Result<()> {
        let new_volume = (self.player_info.volume - 0.05).max(0.0);
        self.player_cmd_tx.send(PlayerCommand::SetVolume(new_volume))?;
        self.player_info.volume = new_volume;
        
        // Save volume to config
        self.config.update_session_state(
            new_volume,
            self.config.last_station_name.clone(),
            self.config.last_station_url.clone(),
        );
        let _ = self.config.save(&self.data_dir);
        
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
                self.browse_stations = self.stations.clone(); // Cache for Browse tab
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
        self.add_log("Loading popular stations...".to_string());
        
        let cache_key = "popular";
        let stations = if let Some(cached) = self.cache.get(cache_key) {
            tracing::info!("Using cached popular stations: {} stations", cached.len());
            self.add_log(format!("Loaded {} stations from cache", cached.len()));
            cached
        } else {
            tracing::info!("Fetching popular stations from API...");
            self.add_log("Fetching stations from API...".to_string());
            match self.api_client.get_popular_stations(self.config.station_limit).await {
                Ok(stations) => {
                    tracing::info!("Fetched {} popular stations from API", stations.len());
                    self.add_log(format!("Fetched {} stations from API", stations.len()));
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
                    self.add_log(msg);
                    return Err(e);
                }
            }
        };

        self.stations = stations;
        self.browse_stations = self.stations.clone(); // Cache for Browse tab
        self.selected_index = 0;
        self.loading = false;
        let msg = format!("Loaded {} popular stations", self.stations.len());
        self.status_message = Some(msg.clone());
        self.add_log(msg);
        tracing::info!("Popular stations loaded successfully: {} stations", self.stations.len());
        
        Ok(())
    }

    pub fn reload_current_tab(&mut self) {
        match self.current_tab {
            Tab::Browse => {
                // Restore cached browse stations
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
}
