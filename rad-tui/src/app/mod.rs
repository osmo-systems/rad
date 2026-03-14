mod favorites;
mod navigation;
mod playback;
mod search;

use anyhow::Result;
use std::collections::HashMap;
use std::path::PathBuf;

use rad_core::{
    RadioBrowserClient, Station, Config, PlayerInfo,
    StartupTab,
    search::{AutocompleteData, SearchQuery},
    storage::{AutovoteManager, CacheManager, FavoritesManager, HistoryManager, SearchHistoryManager, VoteManager},
    PlayerState,
};
use crate::ui::SearchPopup;
use tui_kit::{LogEntry, LogLevel, Toast, ToastLevel};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tab {
    Browse,
    Favorites,
    History,
    Autovote,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HelpTab {
    Keys,
    Settings,
    Log,
}

/// Target of a pending deletion confirmation.
#[derive(Debug, Clone)]
pub enum ConfirmDelete {
    Favorite(String, String), // uuid, name
    Autovote(String, String), // uuid, name
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(dead_code)]
pub enum BrowseMode {
    Popular,
    Search,
    ByCountry,
    ByGenre,
    ByLanguage,
}

/// Which focusable widget currently has keyboard focus on the main screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedWidget {
    StationList,
}

pub struct App {
    pub running: bool,
    pub current_tab: Tab,
    pub browse_mode: BrowseMode,

    // Station lists
    pub stations: Vec<Station>,
    pub browse_stations: Vec<Station>,
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
    pub highest_page_loaded: usize,

    // Player
    pub player_info: PlayerInfo,

    // API & Storage
    pub api_client: RadioBrowserClient,
    pub favorites: FavoritesManager,
    pub history: HistoryManager,
    pub autovote: AutovoteManager,
    #[allow(dead_code)]
    pub cache: CacheManager,
    pub search_history: SearchHistoryManager,
    pub vote_manager: VoteManager,
    pub config: Config,
    pub data_dir: PathBuf,

    // Loading state
    pub loading: bool,
    pub status_message: Option<String>,
    pub pending_search: bool,
    pub pending_page_change: Option<i32>,

    // Error popup
    pub error_popup: Option<String>,

    // Warning popup
    pub warning_popup: Option<String>,

    // Confirm delete popup (favorites / autovote)
    pub confirm_delete: Option<ConfirmDelete>,

    // Which widget currently has keyboard focus
    pub focused_widget: FocusedWidget,

    // Autovote section selection (within Favorites tab)
    pub autovote_selected: usize,

    // Help popup
    pub help_popup: bool,
    pub help_tab: HelpTab,
    pub settings_selected: usize,

    // Lists for browse modes
    pub countries: Vec<String>,
    pub genres: Vec<String>,
    pub languages: Vec<String>,
    pub browse_list_index: usize,
    pub browse_list_mode: bool,

    // Animation
    pub animation_frame: usize,

    // Status log
    pub status_log: Vec<LogEntry>,
    pub help_log_scroll: usize,
    pub log_level_filter: Option<LogLevel>,

    // UI state
    pub visible_stations_count: usize,

    // Toast notifications
    pub toasts: Vec<Toast>,
}

impl App {
    pub async fn new(
        data_dir: PathBuf,
        mut api_client: RadioBrowserClient,
    ) -> Result<Self> {
        let config = Config::load(&data_dir)?;
        let favorites = FavoritesManager::new(&data_dir)?;
        let history = HistoryManager::new(&data_dir, config.max_history_entries)?;
        let autovote = AutovoteManager::new(&data_dir)?;
        let cache = CacheManager::new(&data_dir, config.cache_duration_secs)?;
        let search_history = SearchHistoryManager::new(&data_dir)?;
        let vote_manager = VoteManager::new(&data_dir)?;

        let restored_volume = config.last_volume.unwrap_or(config.default_volume);
        let player_info = PlayerInfo {
            state: PlayerState::Stopped,
            station_name: config.last_station_name.clone().unwrap_or_default(),
            station_url: config.last_station_url.clone().unwrap_or_default(),
            volume: restored_volume,
            error_message: None,
        };

        let autocomplete_data = AutocompleteData::load(&mut api_client).await
            .unwrap_or_default();

        let mut current_query = SearchQuery::default();
        current_query.order = Some(config.default_search_order.as_api_str().to_string());

        let startup_tab = match config.startup_tab {
            StartupTab::Search => Tab::Browse,
            StartupTab::Favorites => Tab::Favorites,
            StartupTab::History => Tab::History,
        };

        let initial_log = Self::load_persisted_log(&data_dir);

        let app = Self {
            running: true,
            current_tab: startup_tab,
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
            autovote,
            cache,
            search_history,
            vote_manager,
            config,
            data_dir,

            loading: false,
            pending_search: false,
            pending_page_change: None,
            status_message: None,

            error_popup: None,
            warning_popup: None,
            confirm_delete: None,
            focused_widget: FocusedWidget::StationList,
            autovote_selected: 0,
            help_popup: false,
            help_tab: HelpTab::Keys,
            settings_selected: 0,

            countries: Vec::new(),
            genres: Vec::new(),
            languages: Vec::new(),
            browse_list_index: 0,
            browse_list_mode: false,

            animation_frame: 0,

            status_log: initial_log,
            help_log_scroll: 0,
            log_level_filter: None,

            visible_stations_count: 10,

            toasts: Vec::new(),
        };

        Ok(app)
    }

    pub fn show_toast(&mut self, message: String, level: ToastLevel) {
        if self.config.toast_duration_secs == 0 {
            return;
        }
        let duration_ms = self.config.toast_duration_secs * 1000;
        self.toasts.push(Toast::new(message, level, duration_ms));
    }

    pub fn tick_toasts(&mut self) {
        self.toasts.retain(|t| !t.is_expired());
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

    pub fn quit(&mut self) {
        self.running = false;
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        let cutoff = std::time::Duration::from_secs(24 * 3600);
        self.status_log.retain(|e| e.created_at.elapsed() < cutoff);

        let timestamp = chrono::Local::now().format("%H:%M:%S").to_string();
        self.status_log.push(LogEntry {
            timestamp,
            level,
            message,
            created_at: std::time::Instant::now(),
        });
        if self.status_log.len() > 100 {
            self.status_log.remove(0);
        }
        self.help_log_scroll = self.status_log.len().saturating_sub(1);
    }

    /// Returns true if any popup is currently open (focus-stealing).
    pub fn has_popup(&self) -> bool {
        self.help_popup
            || self.search_popup.is_some()
            || self.error_popup.is_some()
            || self.warning_popup.is_some()
            || self.confirm_delete.is_some()
    }

    /// Load log entries persisted from previous sessions (last 24 h only).
    pub fn load_persisted_log(data_dir: &std::path::Path) -> Vec<LogEntry> {
        use std::time::{SystemTime, UNIX_EPOCH};
        let path = data_dir.join("rad-tui-log.txt");
        let content = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        content
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.splitn(4, '|').collect();
                if parts.len() != 4 {
                    return None;
                }
                let unix: u64 = parts[0].parse().ok()?;
                if now.saturating_sub(unix) > 24 * 3600 {
                    return None;
                }
                let level = match parts[1] {
                    "D" => LogLevel::Debug,
                    "I" => LogLevel::Info,
                    "W" => LogLevel::Warning,
                    "E" => LogLevel::Error,
                    _ => return None,
                };
                Some(LogEntry {
                    timestamp: parts[2].to_string(),
                    level,
                    message: parts[3].to_string(),
                    created_at: std::time::Instant::now(),
                })
            })
            .collect()
    }

    /// Persist the current in-memory log to disk for the next session.
    pub fn save_log(&self) {
        use std::time::{SystemTime, UNIX_EPOCH};
        let path = self.data_dir.join("rad-tui-log.txt");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let content = self
            .status_log
            .iter()
            .map(|e| {
                let level = match e.level {
                    LogLevel::Debug => "D",
                    LogLevel::Info => "I",
                    LogLevel::Warning => "W",
                    LogLevel::Error => "E",
                };
                let entry_unix = now.saturating_sub(e.created_at.elapsed().as_secs());
                format!("{}|{}|{}|{}", entry_unix, level, e.timestamp, e.message)
            })
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(&path, content);
    }
}
