//! LazyRadio Headless Player Daemon
//! 
//! A lightweight, persistent audio player process that communicates with TUI/CLI clients
//! via Unix socket IPC. This daemon continues playing music even after the client disconnects.

use anyhow::Result;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::Mutex;
use tracing::{info, error, debug};
use tracing_subscriber;

use lazyradio::{
    config::get_data_dir,
    player::{AudioPlayer, PlayerCommand},
    ClientMessage, DaemonMessage,
};

const DAEMON_SOCKET: &str = ".lazyradio-player.sock";

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "lazyradio-daemon.log");
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
    let (audio_player, _player_cmd_rx) = match AudioPlayer::new() {
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

    let player = Arc::new(Mutex::new(audio_player));

    // Create Unix socket listener
    let listener = UnixListener::bind(&socket_path)?;
    info!("Player daemon listening on: {}", socket_path.display());

    // Main accept loop
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                info!("New client connected");
                let player_ref = player.clone();

                // Process this client synchronously within an async context
                if let Err(e) = handle_client(stream, player_ref).await {
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
