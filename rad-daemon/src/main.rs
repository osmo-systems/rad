//! Headless Player Daemon
//!
//! A lightweight, persistent audio player process that communicates with TUI/CLI clients
//! via Unix socket IPC. This daemon continues playing music even after the client disconnects.
//! 
//! The daemon auto-shuts down after 30 minutes of inactivity (when not playing).

use anyhow::Result;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{info, error, debug};
use tracing_subscriber;

use rad_core::{
    config::{get_data_dir, Config},
    player::{AudioPlayer, PlayerCommand, PlayerState},
    ClientMessage, DaemonMessage,
};

const DAEMON_SOCKET: &str = ".radm-player.sock";
const IDLE_TIMEOUT_SECS: u64 = 30 * 60; // 30 minutes

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Initialize logging
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "rad-daemon.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    info!("Player daemon starting...");

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

    // Shared flag: handle_client sets this after every command so the idle
    // task can check playback state without holding the AudioPlayer lock
    // (AudioPlayer is not Send, so it cannot cross thread boundaries).
    let is_playing = Arc::new(AtomicBool::new(false));
    let is_playing_idle = is_playing.clone();

    // Run everything in a LocalSet so non-Send types (AudioPlayer) can be shared
    // across tasks without requiring Send. All tasks execute cooperatively on the
    // single current_thread runtime — no cross-thread movement ever occurs.
    let local = tokio::task::LocalSet::new();
    local.run_until(async move {
        // Spawn idle timeout monitor task.
        // The countdown only advances while the daemon is not playing — if music
        // is playing the timer is reset so the daemon stays alive indefinitely.
        tokio::task::spawn_local(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;

                if is_playing_idle.load(Ordering::Relaxed) {
                    *last_activity_check.lock().await = Instant::now();
                    continue;
                }

                let elapsed = last_activity_check.lock().await.elapsed();
                if elapsed > Duration::from_secs(IDLE_TIMEOUT_SECS) {
                    info!("Daemon idle for {} seconds, shutting down", elapsed.as_secs());
                    std::process::exit(0);
                }
            }
        });

        // Main accept loop — each client gets its own task so connections are
        // handled concurrently; no client blocks another from being accepted.
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    info!("New client connected");
                    let player_ref = player.clone();
                    let last_activity_ref = last_activity.clone();
                    let is_playing_ref = is_playing.clone();

                    tokio::task::spawn_local(async move {
                        if let Err(e) = handle_client(stream, player_ref, last_activity_ref, is_playing_ref).await {
                            error!("Client handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Accept error: {}", e);
                }
            }
        }
    }).await;

    Ok(())
}

async fn handle_client(
    stream: UnixStream,
    player: Arc<Mutex<AudioPlayer>>,
    last_activity: Arc<Mutex<Instant>>,
    is_playing: Arc<AtomicBool>,
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
                is_playing.store(
                    matches!(info.state, PlayerState::Playing | PlayerState::Loading),
                    Ordering::Relaxed,
                );
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
                    is_playing.store(
                        matches!(info.state, PlayerState::Playing | PlayerState::Loading),
                        Ordering::Relaxed,
                    );
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
