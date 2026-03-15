#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
    Loading,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerInfo {
    pub state: PlayerState,
    pub station_name: String,
    pub station_url: String,
    pub volume: f32,
    pub error_message: Option<String>,
}

pub enum PlayerCommand {
    Play(String, String), // (station_name, url)
    Pause,
    Resume,
    Stop,
    SetVolume(f32),
    Reload,
    ClearError,
}
