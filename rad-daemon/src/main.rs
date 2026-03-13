//! Headless Player Daemon
//!
//! A lightweight, persistent audio player process that communicates with TUI/CLI clients
//! via Unix socket IPC. This daemon continues playing music even after the client disconnects.
//!
//! The daemon auto-shuts down after 30 minutes of inactivity (when not playing).

mod handler;

use anyhow::Result;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tracing::{info, error};
use tracing_subscriber;

use rad_core::{
    config::{get_data_dir, Config},
    player::AudioPlayer,
};

const DAEMON_SOCKET: &str = ".radm-player.sock";
const IDLE_TIMEOUT_SECS: u64 = 30 * 60; // 30 minutes

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "rad-daemon.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    info!("Player daemon starting...");

    let socket_path = data_dir.join(DAEMON_SOCKET);

    if socket_path.exists() {
        fs::remove_file(&socket_path)?;
    }

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

    if let Ok(config) = Config::load(&data_dir) {
        if let Some(saved_volume) = config.last_volume {
            info!("Restoring saved volume: {:.2}", saved_volume);
            audio_player.set_volume(saved_volume);
        }
    }

    let player = Arc::new(Mutex::new(audio_player));

    let listener = UnixListener::bind(&socket_path)?;
    info!("Player daemon listening on: {}", socket_path.display());

    let last_activity = Arc::new(Mutex::new(Instant::now()));
    let last_activity_check = last_activity.clone();

    let is_playing = Arc::new(AtomicBool::new(false));
    let is_playing_idle = is_playing.clone();

    // Run everything in a LocalSet so non-Send types (AudioPlayer) can be shared
    // across tasks without requiring Send.
    let local = tokio::task::LocalSet::new();
    local.run_until(async move {
        // Idle timeout monitor: only counts down while not playing.
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

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    info!("New client connected");
                    let player_ref = player.clone();
                    let last_activity_ref = last_activity.clone();
                    let is_playing_ref = is_playing.clone();

                    tokio::task::spawn_local(async move {
                        if let Err(e) = handler::handle_client(stream, player_ref, last_activity_ref, is_playing_ref).await {
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
