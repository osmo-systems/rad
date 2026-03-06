use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use lazyradio::{
    RadioBrowserClient, Station, Config, PlayerInfo,
    search::{AutocompleteData, SearchQuery, format_query, is_default_query},
    storage::{CacheManager, FavoritesManager, HistoryManager, SearchHistoryManager},
    PlayerState, PlayerDaemonConnection,
};
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
    pub highest_page_loaded: usize, // Track the highest page we've successfully loaded
    
    // Player
    pub player_info: PlayerInfo,
    
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
    pub pending_page_change: Option<i32>, // +1 for next, -1 for prev
    
    // Error popup
    pub error_popup: Option<String>,
    
    // Warning popup
    pub warning_popup: Option<String>,
    
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
    
    // UI state
    pub visible_stations_count: usize,
}

impl App {
    pub async fn new(
        data_dir: PathBuf,
        mut api_client: RadioBrowserClient,
        _daemon_conn: &mut PlayerDaemonConnection,
    ) -> Result<Self> {
        let config = Config::load(&data_dir)?;
        let favorites = FavoritesManager::new(&data_dir)?;
        let history = HistoryManager::new(&data_dir, config.max_history_entries)?;
        let cache = CacheManager::new(&data_dir, config.cache_duration_secs)?;
        let search_history = SearchHistoryManager::new(&data_dir)?;
        
        // Restore session state from config
        let restored_volume = config.last_volume.unwrap_or(config.default_volume);
        let player_info = PlayerInfo {
            state: PlayerState::Stopped,
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
            highest_page_loaded: 0,
            
            player_info,
            
            api_client,
            favorites,
            history,
            cache,
            search_history,
            config,
            data_dir,
            
            loading: false,
            pending_search: false,
            pending_page_change: None,
            status_message: None,
            
            error_popup: None,
            warning_popup: None,
            help_popup: false,
            
            countries: Vec::new(),
            genres: Vec::new(),
            languages: Vec::new(),
            browse_list_index: 0,
            browse_list_mode: false,
            
            animation_frame: 0,
            
            status_log: Vec::new(),
            status_log_scroll: 0,
            
            visible_stations_count: 10, // Default, will be updated by UI
        };
        
        Ok(app)
    }
    
    pub fn show_error(&mut self, message: String) {
        tracing::info!("show_error called with: {}", message);
        self.error_popup = Some(message);
    }
    
    pub fn show_warning(&mut self, message: String) {
        tracing::info!("show_warning called with: {}", message);
        self.warning_popup = Some(message);
    }
    
    pub fn close_error_popup(&mut self) {
        tracing::info!("close_error_popup called, was: {:?}", self.error_popup);
        self.error_popup = None;
        self.warning_popup = None;
        tracing::info!("close_error_popup done, error cleared");
    }

    // Search popup methods

    pub fn open_search_popup(&mut self) {
        use lazyradio::search::get_suggestions;
        
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
        self.highest_page_loaded = 0;
        
        tracing::info!("execute_search: Cache cleared, calling API");
        
        // Load first page
        self.loading = true;
        match self.api_client.advanced_search(&query).await {
            Ok(stations) => {
                tracing::info!("execute_search: API returned {} stations", stations.len());
                tracing::info!("execute_search: query.limit = {}", query.limit);
                self.is_last_page = stations.len() < query.limit;
                tracing::info!("execute_search: is_last_page = {}", self.is_last_page);
                
                // Track that we've loaded page 1
                if !stations.is_empty() {
                    self.highest_page_loaded = 1;
                }
                
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
                
                let msg = format!("Found {} stations (is_last_page={})", self.stations.len(), self.is_last_page);
                self.status_message = Some(msg.clone());
                self.add_log(msg);
                
                tracing::info!("execute_search: Log added");
                
                // Close popup if it was open
                if self.search_popup.is_some() {
                    self.close_search_popup();
                }
                
                tracing::info!("execute_search: Completed successfully, highest_page={}", self.highest_page_loaded);
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
        
        tracing::info!("load_page: Loading page {}, current_page={}, is_last_page={}", page, self.current_page, self.is_last_page);
        
        // Check cache first
        if let Some(stations) = self.pages_cache.get(&page) {
            tracing::info!("load_page: Found page {} in cache with {} stations", page, stations.len());
            self.stations = stations.clone();
            self.current_page = page;
            self.selected_index = 0;
            self.scroll_offset = 0;
            
            // Determine if this is the last page:
            // 1. If this cached page has < limit stations, it's definitely the last page
            // 2. If we have the next page in cache, it's not the last page
            // 3. If this page number >= highest_page_loaded and it has full stations, we don't know yet
            let query = &self.current_query;
            if stations.len() < query.limit {
                // This page didn't fill up, so there are no more pages
                self.is_last_page = true;
            } else if self.pages_cache.contains_key(&(page + 1)) {
                // We have the next page cached, so there are more pages
                self.is_last_page = false;
            } else if page >= self.highest_page_loaded {
                // We haven't loaded beyond this page yet, assume there might be more
                self.is_last_page = false;
            }
            // If page < highest_page_loaded, keep is_last_page unchanged (we've been further)
            
            let msg = format!("Loaded page {} from cache ({} stations)", page, stations.len());
            self.status_message = Some(msg.clone());
            self.add_log(msg);
            
            tracing::info!("load_page: Cache loaded, is_last_page={}", self.is_last_page);
            
            return Ok(());
        }
        
        // Not in cache, fetch from API
        let mut query = self.current_query.clone();
        query.offset = (page - 1) * query.limit;
        
        tracing::info!("load_page: Page {} not in cache, fetching from API with offset={}", page, query.offset);
        
        self.loading = true;
        match self.api_client.advanced_search(&query).await {
            Ok(stations) => {
                tracing::info!("load_page: API returned {} stations for page {}", stations.len(), page);
                self.is_last_page = stations.len() < query.limit;
                
                // Track the highest page we've successfully loaded
                if page > self.highest_page_loaded && !stations.is_empty() {
                    self.highest_page_loaded = page;
                    tracing::info!("load_page: Updated highest_page_loaded to {}", self.highest_page_loaded);
                }
                
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
                
                let msg = format!("Loaded page {} ({} stations)", page, self.stations.len());
                self.status_message = Some(msg.clone());
                self.add_log(msg);
                
                tracing::info!("load_page: Success, is_last_page={}, highest_page={}", self.is_last_page, self.highest_page_loaded);
                
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
        tracing::info!("next_page: called, is_last_page={}, current_page={}", self.is_last_page, self.current_page);
        if !self.is_last_page {
            tracing::info!("next_page: calling load_page({})", self.current_page + 1);
            self.load_page(self.current_page + 1).await
        } else {
            tracing::info!("next_page: blocked because is_last_page=true");
            self.add_log("Cannot go to next page: already on last page".to_string());
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

            // Send play command to daemon
            tracing::info!("Sending play command to daemon");
            self.add_log(format!("Playing: {}", station.name));
            match daemon_conn.play(station.name.clone(), station.url_resolved.clone()).await {
                Ok(_) => {
                    self.status_message = Some(format!("Playing: {}", station.name));
                    tracing::info!("Play command sent successfully to daemon");
                    
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
        // Play the restored station from player_info (used when restarting with last station)
        if !self.player_info.station_url.is_empty() {
            tracing::info!("Playing restored station: {}", self.player_info.station_name);
            daemon_conn.play(
                self.player_info.station_name.clone(),
                self.player_info.station_url.clone(),
            ).await?;
            self.add_log(format!("Playing: {}", self.player_info.station_name));
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
        
        // Save volume to config
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
