//! rad-core
//!
//! Core library modules for the rad radio player application.
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
pub use config::{Config, DefaultSearchOrder, StartupTab};
pub use player::{AudioPlayer, PlayerCommand, PlayerInfo, PlayerState};
pub use search::{AutocompleteData, SearchQuery};
pub use storage::{CacheManager, FavoritesManager, HistoryManager, SearchHistoryManager, VoteManager};
pub use ipc::{ClientMessage, DaemonMessage, PlayerStateDto};
pub use ipc_client::{PlayerDaemonClient, PlayerDaemonConnection, DaemonSubscription};
