//! Embedded player daemon.
//!
//! Runs when invoked as `rad --daemon`. Uses a single-threaded runtime +
//! LocalSet to host the non-Send AudioPlayer alongside async networking.
//!
//! Architecture:
//! - One `broadcast::Sender<PlayerInfo>` acts as the state event bus.
//! - Clients that send `Subscribe` enter a bidirectional select! loop:
//!   they can send commands (play/pause/…) and receive pushed StateUpdates.
//! - Clients that send other messages get a one-shot request/response.
//! - A background poller broadcasts player state every 250 ms so that
//!   internal transitions (Loading → Playing, Playing → Error) propagate
//!   to subscribers without needing an explicit command.

use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use rad_core::{
    config::{cleanup_old_logs, get_data_dir, Config},
    ipc::{ClientMessage, DaemonMessage, PlayerStateDto},
    player::{AudioPlayer, PlayerCommand, PlayerInfo, PlayerState},
};

const DAEMON_SOCKET: &str = ".radm-player.sock";
const IDLE_TIMEOUT_SECS: u64 = 30 * 60;

pub fn run() -> Result<()> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(run_async())
}

async fn run_async() -> Result<()> {
    let data_dir = get_data_dir()?;

    let log_file = tracing_appender::rolling::daily(&data_dir, "rad-daemon.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    if let Err(e) = cleanup_old_logs(&data_dir, 7) {
        tracing::warn!("Failed to clean up old logs: {}", e);
    }

    info!("Player daemon starting...");

    let socket_path = data_dir.join(DAEMON_SOCKET);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let (mut audio_player, _cmd_rx) = match AudioPlayer::new() {
        Ok(pair) => {
            info!("Audio player initialized");
            pair
        }
        Err(e) => {
            error!("Failed to initialize audio device: {}", e);
            eprintln!("Failed to initialize audio device: {}", e);
            return Err(e);
        }
    };

    if let Ok(config) = Config::load(&data_dir) {
        if let Some(vol) = config.last_volume {
            info!("Restoring saved volume: {:.2}", vol);
            audio_player.set_volume(vol);
        }
    }

    let player = Arc::new(Mutex::new(audio_player));
    let listener = UnixListener::bind(&socket_path)?;
    info!("Listening on: {}", socket_path.display());

    let (broadcast_tx, _) = broadcast::channel::<PlayerInfo>(32);
    let last_activity = Arc::new(Mutex::new(Instant::now()));
    let is_playing = Arc::new(AtomicBool::new(false));

    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            // Idle timeout monitor
            {
                let last_activity = last_activity.clone();
                let is_playing = is_playing.clone();
                tokio::task::spawn_local(async move {
                    loop {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                        if is_playing.load(Ordering::Relaxed) {
                            *last_activity.lock().await = Instant::now();
                            continue;
                        }
                        let elapsed = last_activity.lock().await.elapsed();
                        if elapsed > Duration::from_secs(IDLE_TIMEOUT_SECS) {
                            info!("Idle for {} seconds, shutting down", elapsed.as_secs());
                            std::process::exit(0);
                        }
                    }
                });
            }

            // Periodic state poller: broadcasts on any internal state change.
            {
                let player = player.clone();
                let broadcast_tx = broadcast_tx.clone();
                let is_playing = is_playing.clone();
                tokio::task::spawn_local(async move {
                    let mut last_info: Option<PlayerInfo> = None;
                    loop {
                        tokio::time::sleep(Duration::from_millis(250)).await;
                        if broadcast_tx.receiver_count() == 0 {
                            continue;
                        }
                        let info = player.lock().await.get_info();
                        is_playing.store(
                            matches!(info.state, PlayerState::Playing | PlayerState::Loading),
                            Ordering::Relaxed,
                        );
                        if last_info.as_ref() != Some(&info) {
                            last_info = Some(info.clone());
                            let _ = broadcast_tx.send(info);
                        }
                    }
                });
            }

            // Accept loop
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        info!("New client connected");
                        let player = player.clone();
                        let broadcast_tx = broadcast_tx.clone();
                        let last_activity = last_activity.clone();
                        let is_playing = is_playing.clone();
                        tokio::task::spawn_local(async move {
                            if let Err(e) = handle_client(
                                stream,
                                player,
                                broadcast_tx,
                                last_activity,
                                is_playing,
                            )
                            .await
                            {
                                error!("Client handler error: {}", e);
                            }
                        });
                    }
                    Err(e) => error!("Accept error: {}", e),
                }
            }
        })
        .await;

    Ok(())
}

async fn handle_client(
    stream: UnixStream,
    player: Arc<Mutex<AudioPlayer>>,
    broadcast_tx: broadcast::Sender<PlayerInfo>,
    last_activity: Arc<Mutex<Instant>>,
    is_playing: Arc<AtomicBool>,
) -> Result<()> {
    let (read_half, write_half) = tokio::io::split(stream);
    let mut reader = BufReader::new(read_half);
    let mut writer = write_half;

    // Read first message to determine connection mode
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Ok(());
    }
    *last_activity.lock().await = Instant::now();

    let first_msg: ClientMessage = match serde_json::from_str(line.trim()) {
        Ok(msg) => msg,
        Err(e) => {
            send_msg(
                &mut writer,
                &DaemonMessage::Error(format!("Invalid message: {}", e)),
            )
            .await?;
            return Ok(());
        }
    };

    match first_msg {
        ClientMessage::Subscribe => {
            // Long-lived subscription: push StateUpdates, receive commands
            let mut broadcast_rx = broadcast_tx.subscribe();

            // Send immediate state snapshot so TUI starts with current state
            let info = player.lock().await.get_info();
            send_state_update(&mut writer, &info).await?;

            // Spawn a reader task so reads don't block the broadcast select!
            let (cmd_tx, mut cmd_rx) = tokio::sync::mpsc::channel::<ClientMessage>(8);
            tokio::task::spawn_local(async move {
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            if let Ok(msg) = serde_json::from_str::<ClientMessage>(line.trim()) {
                                if cmd_tx.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                }
            });

            loop {
                tokio::select! {
                    msg = cmd_rx.recv() => {
                        match msg {
                            None => break,
                            Some(ClientMessage::Shutdown) => {
                                info!("Subscriber requested shutdown");
                                std::process::exit(0);
                            }
                            Some(msg) => {
                                *last_activity.lock().await = Instant::now();
                                if let Some(cmd) = Option::<PlayerCommand>::from(msg) {
                                    execute_command(&player, &is_playing, cmd).await;
                                    let info = player.lock().await.get_info();
                                    let _ = broadcast_tx.send(info);
                                }
                            }
                        }
                    }
                    result = broadcast_rx.recv() => {
                        match result {
                            Ok(info) => {
                                if send_state_update(&mut writer, &info).await.is_err() {
                                    break;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {}
                            Err(_) => break,
                        }
                    }
                }
            }
        }
        first_msg => {
            // One-shot request/response (CLI clients)
            let response = handle_one_shot(&player, &is_playing, first_msg).await;
            send_msg(&mut writer, &response).await?;
            if matches!(response, DaemonMessage::Shutdown) {
                std::process::exit(0);
            }

            loop {
                line.clear();
                if reader.read_line(&mut line).await? == 0 {
                    break;
                }
                *last_activity.lock().await = Instant::now();

                let msg: ClientMessage = match serde_json::from_str(line.trim()) {
                    Ok(m) => m,
                    Err(e) => {
                        send_msg(
                            &mut writer,
                            &DaemonMessage::Error(format!("Invalid message: {}", e)),
                        )
                        .await?;
                        continue;
                    }
                };

                let response = handle_one_shot(&player, &is_playing, msg).await;
                send_msg(&mut writer, &response).await?;
                if matches!(response, DaemonMessage::Shutdown) {
                    std::process::exit(0);
                }
            }
        }
    }

    info!("Client disconnected");
    Ok(())
}

async fn handle_one_shot(
    player: &Arc<Mutex<AudioPlayer>>,
    is_playing: &Arc<AtomicBool>,
    msg: ClientMessage,
) -> DaemonMessage {
    match msg {
        ClientMessage::GetStatus => {
            let info = player.lock().await.get_info();
            is_playing.store(
                matches!(info.state, PlayerState::Playing | PlayerState::Loading),
                Ordering::Relaxed,
            );
            player_info_to_state(info)
        }
        ClientMessage::Shutdown => DaemonMessage::Shutdown,
        msg => {
            if let Some(cmd) = Option::<PlayerCommand>::from(msg) {
                execute_command(player, is_playing, cmd).await;
                let info = player.lock().await.get_info();
                player_info_to_state(info)
            } else {
                DaemonMessage::Error("Unknown command".to_string())
            }
        }
    }
}

async fn execute_command(
    player: &Arc<Mutex<AudioPlayer>>,
    is_playing: &Arc<AtomicBool>,
    cmd: PlayerCommand,
) {
    {
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
    }

    // Give the player a moment to transition state
    tokio::time::sleep(Duration::from_millis(50)).await;

    let info = player.lock().await.get_info();
    is_playing.store(
        matches!(info.state, PlayerState::Playing | PlayerState::Loading),
        Ordering::Relaxed,
    );
}

fn player_info_to_state(info: PlayerInfo) -> DaemonMessage {
    DaemonMessage::State {
        state: info.state.into(),
        station_name: info.station_name,
        station_url: info.station_url,
        volume: info.volume,
        error_message: info.error_message,
    }
}

async fn send_state_update(
    writer: &mut tokio::io::WriteHalf<UnixStream>,
    info: &PlayerInfo,
) -> Result<()> {
    send_msg(
        writer,
        &DaemonMessage::StateUpdate {
            state: PlayerStateDto::from(info.state),
            station_name: info.station_name.clone(),
            station_url: info.station_url.clone(),
            volume: info.volume,
            error_message: info.error_message.clone(),
        },
    )
    .await
}

async fn send_msg(writer: &mut tokio::io::WriteHalf<UnixStream>, msg: &DaemonMessage) -> Result<()> {
    let json = serde_json::to_string(msg)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
