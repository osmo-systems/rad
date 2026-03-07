//! LazyRadio Library
//! 
//! Core library modules for the LazyRadio radio player application.
//! Provides radio browsing, searching, playing, and management functionality.

pub mod api;
pub mod config;
pub mod player;
pub mod search;
pub mod storage;
pub mod ipc;
pub mod ipc_client;

// Re-export commonly used types
pub use api::{RadioBrowserClient, Station};
pub use config::Config;
pub use player::{AudioPlayer, PlayerCommand, PlayerInfo, PlayerState};
pub use search::{AutocompleteData, SearchQuery};
pub use storage::{CacheManager, FavoritesManager, HistoryManager, SearchHistoryManager};
pub use ipc::{ClientMessage, DaemonMessage, PlayerStateDto};
pub use ipc_client::{PlayerDaemonClient, PlayerDaemonConnection};
