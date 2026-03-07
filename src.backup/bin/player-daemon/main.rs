//! LazyRadio Headless Player Daemon
//! 
//! A lightweight, persistent audio player process that communicates with TUI/CLI clients
//! via Unix socket IPC. This daemon continues playing music even after the client disconnects.
//! 
//! The daemon auto-shuts down after 30 minutes of inactivity (when not playing).

use anyhow::Result;
use std::fs;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{info, error, debug};
use tracing_subscriber;

use radm::{
    config::{get_data_dir, Config},
    player::{AudioPlayer, PlayerCommand},
    ClientMessage, DaemonMessage,
};

const DAEMON_SOCKET: &str = ".radm-player.sock";
const IDLE_TIMEOUT_SECS: u64 = 30 * 60; // 30 minutes

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "rad-daemon.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    info!("LazyRadio Player Daemon starting...");

    // Get socket path
    let socket_path = data_dir.join(DAEMON_SOCKET);
    
    // Remove old socket if it exists
    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

    // Initialize audio player
    let (mut audio_player, _player_cmd_rx) = match AudioPlayer::new() {
        Ok((player, rx)) => {
            info!("Audio player initialized");
            (player, rx)
        }
        Err(e) => {
            error!("Failed to initialize audio device: {}", e);
            eprintln!("Failed to initialize audio device: {}", e);
            return Err(e);
        }
    };

    // Load saved volume from config if available
    if let Ok(config) = Config::load(&data_dir) {
        if let Some(saved_volume) = config.last_volume {
            info!("Restoring saved volume: {:.2}", saved_volume);
            audio_player.set_volume(saved_volume);
        }
    }

    let player = Arc::new(Mutex::new(audio_player));

    // Create Unix socket listener
    let listener = UnixListener::bind(&socket_path)?;
    info!("Player daemon listening on: {}", socket_path.display());

    // Track last activity time for idle timeout
    let last_activity = Arc::new(Mutex::new(Instant::now()));
    let last_activity_check = last_activity.clone();

    // Spawn idle timeout monitor task
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await; // Check every minute
            
            let last_activity_time = *last_activity_check.lock().await;
            let elapsed = last_activity_time.elapsed();
            
            if elapsed > Duration::from_secs(IDLE_TIMEOUT_SECS) {
                info!("Daemon idle for {} seconds, shutting down", elapsed.as_secs());
                std::process::exit(0);
            }
        }
    });

    // Main accept loop
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                info!("New client connected");
                let player_ref = player.clone();
                let last_activity_ref = last_activity.clone();

                // Process this client synchronously within an async context
                if let Err(e) = handle_client(stream, player_ref, last_activity_ref).await {
                    error!("Client handler error: {}", e);
                }
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }
}

async fn handle_client(
    stream: UnixStream,
    player: Arc<Mutex<AudioPlayer>>,
    last_activity: Arc<Mutex<Instant>>,
) -> Result<()> {
    let (reader, writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut writer = writer;
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).await?;
        
        if n == 0 {
            info!("Client disconnected");
            break;
        }

        // Update last activity time
        {
            let mut last_activity_time = last_activity.lock().await;
            *last_activity_time = Instant::now();
        }

        // Parse message
        let message: ClientMessage = match serde_json::from_str(&line) {
            Ok(msg) => msg,
            Err(e) => {
                error!("Failed to parse message: {}", e);
                let response = DaemonMessage::Error(format!("Invalid message format: {}", e));
                send_response(&mut writer, &response).await?;
                continue;
            }
        };

        debug!("Received message: {:?}", message);

        // Handle message
        let response = match message {
            ClientMessage::GetStatus => {
                let s = player.lock().await;
                let info = s.get_info();
                DaemonMessage::Status {
                    state: info.state.into(),
                    station_name: info.station_name,
                    station_url: info.station_url,
                    volume: info.volume,
                    error_message: info.error_message,
                }
            }
            ClientMessage::Shutdown => {
                info!("Shutdown requested by client");
                DaemonMessage::Shutdown
            }
            msg => {
                // Convert to PlayerCommand and execute it
                if let Some(cmd) = Option::<PlayerCommand>::from(msg.clone()) {
                    let mut p = player.lock().await;
                    match cmd {
                        PlayerCommand::Play(name, url) => p.play(name, url),
                        PlayerCommand::Pause => p.pause(),
                        PlayerCommand::Resume => p.resume(),
                        PlayerCommand::Stop => p.stop(),
                        PlayerCommand::SetVolume(vol) => p.set_volume(vol),
                        PlayerCommand::Reload => p.reload(),
                        PlayerCommand::ClearError => p.clear_error(),
                    }
                    
                    // Small delay to let command propagate
                    drop(p);
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    
                    // Get updated status
                    let s = player.lock().await;
                    let info = s.get_info();
                    DaemonMessage::Status {
                        state: info.state.into(),
                        station_name: info.station_name,
                        station_url: info.station_url,
                        volume: info.volume,
                        error_message: info.error_message,
                    }
                } else {
                    DaemonMessage::Error("Unknown command".to_string())
                }
            }
        };

        send_response(&mut writer, &response).await?;

        if matches!(response, DaemonMessage::Shutdown) {
            break;
        }
    }

    Ok(())
}

async fn send_response(
    writer: &mut tokio::io::WriteHalf<UnixStream>,
    response: &DaemonMessage,
) -> Result<()> {
    let json = serde_json::to_string(response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}
