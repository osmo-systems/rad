//! IPC Protocol for Player Daemon
//!
//! Defines message types and serialization for communication between
//! TUI/CLI clients and the headless player daemon over Unix sockets.

use crate::player::{PlayerCommand, PlayerState};
use serde::{Deserialize, Serialize};

/// Messages sent FROM client TO daemon
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ClientMessage {
    /// Play a station
    Play { station_name: String, url: String },
    /// Pause playback
    Pause,
    /// Resume playback
    Resume,
    /// Stop playback
    Stop,
    /// Set volume (0.0 to 1.0)
    SetVolume(f32),
    /// Reload current station
    Reload,
    /// Clear error state
    ClearError,
    /// Get current player status
    GetStatus,
    /// Graceful shutdown
    Shutdown,
}

/// Messages sent FROM daemon TO client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DaemonMessage {
    /// Status update with current player info
    Status {
        state: PlayerStateDto,
        station_name: String,
        station_url: String,
        volume: f32,
        error_message: Option<String>,
    },
    /// Acknowledgement of command
    Ok,
    /// Error response
    Error(String),
    /// Daemon is shutting down
    Shutdown,
}

/// Serializable version of PlayerState for JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlayerStateDto {
    Stopped,
    Playing,
    Paused,
    Loading,
    Error,
}

impl From<PlayerState> for PlayerStateDto {
    fn from(state: PlayerState) -> Self {
        match state {
            PlayerState::Stopped => PlayerStateDto::Stopped,
            PlayerState::Playing => PlayerStateDto::Playing,
            PlayerState::Paused => PlayerStateDto::Paused,
            PlayerState::Loading => PlayerStateDto::Loading,
            PlayerState::Error => PlayerStateDto::Error,
        }
    }
}

impl From<PlayerStateDto> for PlayerState {
    fn from(state: PlayerStateDto) -> Self {
        match state {
            PlayerStateDto::Stopped => PlayerState::Stopped,
            PlayerStateDto::Playing => PlayerState::Playing,
            PlayerStateDto::Paused => PlayerState::Paused,
            PlayerStateDto::Loading => PlayerState::Loading,
            PlayerStateDto::Error => PlayerState::Error,
        }
    }
}

/// Convert ClientMessage to PlayerCommand (for daemon internal use)
impl From<ClientMessage> for Option<PlayerCommand> {
    fn from(msg: ClientMessage) -> Self {
        match msg {
            ClientMessage::Play { station_name, url } => {
                Some(PlayerCommand::Play(station_name, url))
            }
            ClientMessage::Pause => Some(PlayerCommand::Pause),
            ClientMessage::Resume => Some(PlayerCommand::Resume),
            ClientMessage::Stop => Some(PlayerCommand::Stop),
            ClientMessage::SetVolume(vol) => Some(PlayerCommand::SetVolume(vol)),
            ClientMessage::Reload => Some(PlayerCommand::Reload),
            ClientMessage::ClearError => Some(PlayerCommand::ClearError),
            ClientMessage::GetStatus => None,
            ClientMessage::Shutdown => None,
        }
    }
}
