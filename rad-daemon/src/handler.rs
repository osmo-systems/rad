use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{debug, error, info};

use rad_core::{
    player::{AudioPlayer, PlayerCommand, PlayerState},
    ClientMessage, DaemonMessage,
};

pub async fn handle_client(
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

        {
            let mut last_activity_time = last_activity.lock().await;
            *last_activity_time = Instant::now();
        }

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

                    drop(p);
                    tokio::time::sleep(Duration::from_millis(50)).await;

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

pub async fn send_response(
    writer: &mut tokio::io::WriteHalf<UnixStream>,
    response: &DaemonMessage,
) -> Result<()> {
    let json = serde_json::to_string(response)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
}
