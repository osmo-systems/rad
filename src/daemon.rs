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
//! - Media controls (headphone buttons, Now Playing widget) are handled
//!   via `souvlaki`. Events arrive on a mpsc channel and are executed in
//!   the same accept loop, then broadcast to all subscribers.

use anyhow::Result;
use souvlaki::{MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, PlatformConfig};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info, warn};

use rad::{
    config::{cleanup_old_logs, get_data_dir, Config},
    ipc::{ClientMessage, DaemonMessage, PlayerStateDto},
    player::{AudioPlayer, PlayerCommand, PlayerInfo, PlayerState},
    FavoritesManager,
};

const DAEMON_SOCKET: &str = ".radm-player.sock";
const IDLE_TIMEOUT_SECS: u64 = 30 * 60;

/// Internal media control events, separate from the IPC protocol.
enum MediaCmd {
    /// A direct command to forward to the player.
    Cmd(ClientMessage),
    /// Toggle play/pause — requires knowing current state, handled in-loop.
    Toggle,
    /// Skip to the next favorited station.
    NextFavorite,
    /// Skip to the previous favorited station.
    PrevFavorite,
}

// On macOS, souvlaki's MPRemoteCommandCenter callbacks are dispatched on the
// GCD main queue.  For them to fire, the main thread must be running
// dispatch_main().  We therefore run the tokio runtime on a background
// thread and give the main thread to GCD.
#[cfg(target_os = "macos")]
extern "C" {
    fn dispatch_main() -> !;
}

pub fn run() -> Result<()> {
    // Rename the process so it shows as "radm" in Activity Monitor / ps,
    // distinguishable from the TUI which shows as "rad".
    #[cfg(target_os = "macos")]
    {
        extern "C" {
            fn setprogname(name: *const std::ffi::c_char);
        }
        unsafe { setprogname(b"radm\0".as_ptr() as *const std::ffi::c_char); }
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::fs::write("/proc/self/comm", "radm");
    }

    #[cfg(target_os = "macos")]
    {
        std::thread::spawn(|| {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build tokio runtime");
            if let Err(e) = rt.block_on(run_async()) {
                eprintln!("Daemon error: {}", e);
            }
            std::process::exit(0);
        });

        // Never returns — gives the main thread to the GCD dispatch queue
        // so that MPRemoteCommandCenter callbacks can fire.
        unsafe { dispatch_main() }
    }

    #[cfg(not(target_os = "macos"))]
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
        warn!("Failed to clean up old logs: {}", e);
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

    // Channel for souvlaki → tokio command delivery.
    // try_send is used in the sync callback so it never blocks.
    let (media_tx, mut media_rx) = tokio::sync::mpsc::channel::<MediaCmd>(8);

    // Initialize media controls (headphone buttons / Now Playing widget).
    // Failure is non-fatal — rad works fine without it.
    let mut controls = init_media_controls(media_tx);

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

            // Periodic state poller: broadcasts on any internal state change
            // (e.g. Loading → Playing, Playing → Error).
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

            // Subscribe to the broadcast to update media controls state.
            let mut controls_rx = broadcast_tx.subscribe();

            // Accept loop — also handles media control events and state updates.
            loop {
                tokio::select! {
                    accept_result = listener.accept() => {
                        match accept_result {
                            Ok((stream, _)) => {
                                info!("New client connected");
                                let player = player.clone();
                                let broadcast_tx = broadcast_tx.clone();
                                let last_activity = last_activity.clone();
                                let is_playing = is_playing.clone();
                                tokio::task::spawn_local(async move {
                                    if let Err(e) = handle_client(
                                        stream, player, broadcast_tx, last_activity, is_playing,
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

                    Some(cmd) = media_rx.recv() => {
                        match cmd {
                            MediaCmd::Toggle => {
                                let state = player.lock().await.get_info().state;
                                let cmd = if state == PlayerState::Playing {
                                    PlayerCommand::Pause
                                } else {
                                    PlayerCommand::Resume
                                };
                                execute_command(&player, &is_playing, cmd).await;
                                let _ = broadcast_tx.send(player.lock().await.get_info());
                            }
                            MediaCmd::Cmd(msg) => {
                                if let Some(cmd) = Option::<PlayerCommand>::from(msg) {
                                    execute_command(&player, &is_playing, cmd).await;
                                    let _ = broadcast_tx.send(player.lock().await.get_info());
                                }
                            }
                            MediaCmd::NextFavorite | MediaCmd::PrevFavorite => {
                                let forward = matches!(cmd, MediaCmd::NextFavorite);
                                if let Some((name, url)) = pick_adjacent_favorite(&data_dir, &player, forward).await {
                                    execute_command(&player, &is_playing, PlayerCommand::Play(name, url)).await;
                                    let _ = broadcast_tx.send(player.lock().await.get_info());
                                }
                            }
                        }
                    }

                    // Keep Now Playing / headphone indicator in sync.
                    Ok(info) = controls_rx.recv() => {
                        if let Some(ref mut c) = controls {
                            update_media_controls(c, &info);
                            #[cfg(target_os = "macos")]
                            update_macos_queue_info(&data_dir, &info.station_url);
                        }
                    }
                }
            }
        })
        .await;

    Ok(())
}

/// Configures the macOS Now Playing widget for station navigation.
///
/// - Enables `nextTrackCommand` / `previousTrackCommand` when there are ≥ 2
///   favorites to navigate between.
/// - Disables `changePlaybackPositionCommand`, `seekForwardCommand`, and
///   `seekBackwardCommand`: souvlaki enables seek-to-position by default, which
///   causes macOS to display seek forward/backward buttons in the widget instead
///   of next/previous track buttons. Those seek commands have no handler so they
///   appear grayed out. Disabling them forces macOS to show the track buttons.
/// - Writes `MPNowPlayingInfoPropertyPlaybackQueueCount` + `…QueueIndex` so the
///   system knows how many tracks exist and shows both buttons as navigable.
///
/// Property keys are NSString literals to avoid a hard link against
/// MediaPlayer.framework (souvlaki loads it via the ObjC runtime only).
#[cfg(target_os = "macos")]
fn update_macos_queue_info(data_dir: &std::path::Path, current_url: &str) {
    use cocoa::base::{id, nil, NO, YES};
    use cocoa::foundation::{NSInteger, NSString};
    use objc::{class, msg_send, sel, sel_impl};

    let favorites = match FavoritesManager::new(&data_dir.to_path_buf()) {
        Ok(f) => f,
        Err(e) => {
            warn!("Failed to load favorites for Now Playing navigation: {}", e);
            return;
        }
    };
    let all = favorites.get_all();
    let has_nav = all.len() >= 2;

    unsafe {
        let command_center: id =
            msg_send!(class!(MPRemoteCommandCenter), sharedCommandCenter);

        let enabled: cocoa::base::BOOL = if has_nav { YES } else { NO };

        let cmd: id = msg_send!(command_center, nextTrackCommand);
        let _: () = msg_send!(cmd, setEnabled: enabled);

        let cmd: id = msg_send!(command_center, previousTrackCommand);
        let _: () = msg_send!(cmd, setEnabled: enabled);

        // Disable seek commands so macOS shows next/prev track buttons instead.
        let cmd: id = msg_send!(command_center, changePlaybackPositionCommand);
        let _: () = msg_send!(cmd, setEnabled: NO);
        let cmd: id = msg_send!(command_center, seekForwardCommand);
        let _: () = msg_send!(cmd, setEnabled: NO);
        let cmd: id = msg_send!(command_center, seekBackwardCommand);
        let _: () = msg_send!(cmd, setEnabled: NO);

        if !has_nav {
            return;
        }

        let count = all.len();
        let index = all
            .iter()
            .position(|f| f.url == current_url)
            .unwrap_or(count / 2);

        let media_center: id =
            msg_send!(class!(MPNowPlayingInfoCenter), defaultCenter);
        let prev_info: id = msg_send!(media_center, nowPlayingInfo);

        let info_dict: id = if prev_info == nil {
            msg_send!(class!(NSMutableDictionary), dictionary)
        } else {
            msg_send!(class!(NSMutableDictionary), dictionaryWithDictionary: prev_info)
        };

        let key_count: id = NSString::alloc(nil)
            .init_str("MPNowPlayingInfoPropertyPlaybackQueueCount");
        let key_index: id = NSString::alloc(nil)
            .init_str("MPNowPlayingInfoPropertyPlaybackQueueIndex");

        let count_num: id =
            msg_send!(class!(NSNumber), numberWithInteger: count as NSInteger);
        let _: () = msg_send!(info_dict, setObject: count_num forKey: key_count);

        let index_num: id =
            msg_send!(class!(NSNumber), numberWithInteger: index as NSInteger);
        let _: () = msg_send!(info_dict, setObject: index_num forKey: key_index);

        let _: () = msg_send!(media_center, setNowPlayingInfo: info_dict);
    }
}

// ---------------------------------------------------------------------------
// Favorites navigation
// ---------------------------------------------------------------------------

/// Returns the (name, url) of the next or previous favorite relative to the
/// currently playing station.  Reloads favorites from disk each call so that
/// changes made in the TUI are picked up immediately.  Wraps around.
async fn pick_adjacent_favorite(
    data_dir: &std::path::Path,
    player: &Arc<Mutex<AudioPlayer>>,
    forward: bool,
) -> Option<(String, String)> {
    let favorites = match FavoritesManager::new(&data_dir.to_path_buf()) {
        Ok(f) => f,
        Err(e) => {
            warn!("Failed to load favorites for media navigation: {}", e);
            return None;
        }
    };

    let all = favorites.get_all();
    if all.is_empty() {
        info!("Media next/prev: no favorites to navigate");
        return None;
    }

    let current_url = player.lock().await.get_info().station_url;
    let current_idx = all.iter().position(|f| f.url == current_url);

    let next_idx = match current_idx {
        Some(i) => {
            if forward {
                (i + 1) % all.len()
            } else {
                i.checked_sub(1).unwrap_or(all.len() - 1)
            }
        }
        // Not currently playing a favorite — start from first or last.
        None => {
            if forward { 0 } else { all.len() - 1 }
        }
    };

    let station = &all[next_idx];
    info!("Media next/prev: navigating to favorite \"{}\"", station.name);
    Some((station.name.clone(), station.url.clone()))
}

// ---------------------------------------------------------------------------
// Media controls
// ---------------------------------------------------------------------------

fn init_media_controls(tx: tokio::sync::mpsc::Sender<MediaCmd>) -> Option<MediaControls> {
    let config = PlatformConfig {
        dbus_name: "rad",
        display_name: "Rad Radio Player",
        hwnd: None,
    };

    let mut controls = match MediaControls::new(config) {
        Ok(c) => c,
        Err(e) => {
            warn!("Media controls unavailable: {:?}", e);
            return None;
        }
    };

    if let Err(e) = controls.attach(move |event: MediaControlEvent| {
        let cmd = match event {
            MediaControlEvent::Play => Some(MediaCmd::Cmd(ClientMessage::Resume)),
            MediaControlEvent::Pause => Some(MediaCmd::Cmd(ClientMessage::Pause)),
            MediaControlEvent::Toggle => Some(MediaCmd::Toggle),
            MediaControlEvent::Stop => Some(MediaCmd::Cmd(ClientMessage::Stop)),
            MediaControlEvent::SetVolume(v) => {
                Some(MediaCmd::Cmd(ClientMessage::SetVolume(v as f32)))
            }
            MediaControlEvent::Next => Some(MediaCmd::NextFavorite),
            MediaControlEvent::Previous => Some(MediaCmd::PrevFavorite),
            _ => None,
        };
        if let Some(cmd) = cmd {
            // try_send is non-blocking — safe to call from a system callback.
            let _ = tx.try_send(cmd);
        }
    }) {
        warn!("Failed to attach media controls callback: {:?}", e);
        return None;
    }

    info!("Media controls initialized (headphone buttons active)");
    Some(controls)
}

fn update_media_controls(controls: &mut MediaControls, info: &PlayerInfo) {
    let playback = match info.state {
        PlayerState::Playing | PlayerState::Loading => MediaPlayback::Playing { progress: None },
        PlayerState::Paused => MediaPlayback::Paused { progress: None },
        PlayerState::Stopped | PlayerState::Error => MediaPlayback::Stopped,
    };
    let _ = controls.set_playback(playback);

    let _ = controls.set_metadata(MediaMetadata {
        title: if info.station_name.is_empty() {
            None
        } else {
            Some(&info.station_name)
        },
        ..Default::default()
    });
}

// ---------------------------------------------------------------------------
// Client handling
// ---------------------------------------------------------------------------

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
            let mut broadcast_rx = broadcast_tx.subscribe();

            // Send immediate state snapshot
            let info = player.lock().await.get_info();
            send_state_update(&mut writer, &info).await?;

            // Reader task feeds commands into a mpsc channel so the select!
            // loop below doesn't have to hold a mutable borrow on reader
            // across branches.
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
                                    let _ = broadcast_tx.send(player.lock().await.get_info());
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
                player_info_to_state(player.lock().await.get_info())
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

async fn send_msg(
    writer: &mut tokio::io::WriteHalf<UnixStream>,
    msg: &DaemonMessage,
) -> Result<()> {
    let json = serde_json::to_string(msg)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    Ok(())
}
