//! IPC Client for Player Daemon
//!
//! Provides a client interface for TUI/CLI applications to communicate with
//! the headless player daemon via Unix socket.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::config::get_data_dir;
use crate::ipc::{ClientMessage, DaemonMessage};
use crate::player::PlayerInfo;

const DAEMON_SOCKET: &str = ".radm-player.sock";

/// Client for communicating with the player daemon
pub struct PlayerDaemonClient {
    socket_path: PathBuf,
}

impl PlayerDaemonClient {
    /// Create a new client
    pub fn new() -> Result<Self> {
        let data_dir = get_data_dir()?;
        let socket_path = data_dir.join(DAEMON_SOCKET);
        Ok(Self { socket_path })
    }

    /// Connect for one-shot request/response (used by CLI)
    pub async fn connect(&self) -> Result<PlayerDaemonConnection> {
        if let Ok(stream) = UnixStream::connect(&self.socket_path).await {
            info!("Connected to existing player daemon");
            let mut conn = PlayerDaemonConnection::new(stream).await?;

            // Health-check: verify the daemon responds within 3 seconds.
            match tokio::time::timeout(
                std::time::Duration::from_secs(3),
                conn.get_status(),
            )
            .await
            {
                Ok(Ok(_)) => {
                    info!("Daemon health check passed");
                    return Ok(conn);
                }
                Ok(Err(e)) => {
                    info!("Daemon health check failed ({}), restarting daemon", e);
                }
                Err(_) => {
                    info!("Daemon health check timed out, restarting daemon");
                }
            }

            self.kill_daemon();
        }

        info!("Player daemon not running, attempting to start it...");

        if self.socket_path.exists() {
            info!("Removing stale socket file: {}", self.socket_path.display());
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                tracing::warn!("Failed to remove stale socket file: {}", e);
            }
        }

        self.start_daemon().await?;

        for attempt in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;

            if let Ok(stream) = UnixStream::connect(&self.socket_path).await {
                info!("Connected to newly started player daemon");
                return Ok(PlayerDaemonConnection::new(stream).await?);
            }

            if attempt == 9 {
                return Err(anyhow::anyhow!(
                    "Failed to connect to daemon after starting it"
                ));
            }
        }

        Err(anyhow::anyhow!("Failed to connect to daemon"))
    }

    /// Connect and subscribe to push-based state updates (used by TUI)
    pub async fn subscribe(&self) -> Result<DaemonSubscription> {
        let stream = self.get_or_start_stream().await?;
        let (read_half, write_half) = tokio::io::split(stream);
        let writer = Arc::new(Mutex::new(write_half));

        // Send Subscribe message
        {
            let json = serde_json::to_string(&ClientMessage::Subscribe)?;
            let mut w = writer.lock().await;
            w.write_all(json.as_bytes()).await?;
            w.write_all(b"\n").await?;
            w.flush().await?;
        }

        // Background task: read StateUpdates from socket → mpsc channel
        let (tx, rx) = tokio::sync::mpsc::channel::<PlayerInfo>(32);
        let task = tokio::spawn(async move {
            let mut reader = BufReader::new(read_half);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let info = match serde_json::from_str::<DaemonMessage>(line.trim()) {
                            Ok(DaemonMessage::StateUpdate {
                                state,
                                station_name,
                                station_url,
                                volume,
                                error_message,
                            })
                            | Ok(DaemonMessage::State {
                                state,
                                station_name,
                                station_url,
                                volume,
                                error_message,
                            }) => PlayerInfo {
                                state: state.into(),
                                station_name,
                                station_url,
                                volume,
                                error_message,
                            },
                            _ => continue,
                        };
                        if tx.send(info).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Ok(DaemonSubscription {
            writer,
            state_rx: rx,
            _reader_task: task,
        })
    }

    /// Ensure daemon is running and return a raw UnixStream
    async fn get_or_start_stream(&self) -> Result<UnixStream> {
        if let Ok(stream) = UnixStream::connect(&self.socket_path).await {
            info!("Connected to existing player daemon");
            return Ok(stream);
        }

        info!("Player daemon not running, starting it...");

        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }

        self.start_daemon().await?;

        for attempt in 0..10 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if let Ok(stream) = UnixStream::connect(&self.socket_path).await {
                info!("Connected to newly started player daemon");
                return Ok(stream);
            }
            if attempt == 9 {
                return Err(anyhow::anyhow!(
                    "Failed to connect to daemon after starting it"
                ));
            }
        }

        Err(anyhow::anyhow!("Failed to connect to daemon"))
    }

    /// Kill the running daemon process
    fn kill_daemon(&self) {
        info!("Killing unresponsive daemon");
        let _ = std::process::Command::new("pkill")
            .args(["-f", "rad --daemon"])
            .status();
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    /// Start the daemon process (same binary with --daemon flag)
    async fn start_daemon(&self) -> Result<()> {
        let current_exe = std::env::current_exe()?;
        info!("Starting daemon: {} --daemon", current_exe.display());
        tokio::process::Command::new(&current_exe)
            .arg("--daemon")
            .spawn()
            .context("Failed to start player daemon")?;
        info!("Daemon process spawned");
        Ok(())
    }

    /// Check if daemon is running
    pub async fn is_running(&self) -> bool {
        UnixStream::connect(&self.socket_path).await.is_ok()
    }
}

/// Long-lived subscription connection to the player daemon.
///
/// Sends commands over the write half and receives pushed `PlayerInfo`
/// state updates from a background reader task.
pub struct DaemonSubscription {
    writer: Arc<Mutex<tokio::io::WriteHalf<UnixStream>>>,
    state_rx: tokio::sync::mpsc::Receiver<PlayerInfo>,
    _reader_task: tokio::task::JoinHandle<()>,
}

impl DaemonSubscription {
    /// Send a fire-and-forget command to the daemon.
    pub async fn send_command(&self, msg: ClientMessage) -> Result<()> {
        let json = serde_json::to_string(&msg)?;
        let mut w = self.writer.lock().await;
        w.write_all(json.as_bytes()).await?;
        w.write_all(b"\n").await?;
        w.flush().await?;
        debug!("Sent command: {:?}", msg);
        Ok(())
    }

    /// Receive the next pushed state update (awaitable).
    pub async fn recv(&mut self) -> Option<PlayerInfo> {
        self.state_rx.recv().await
    }
}

/// One-shot request/response connection (used by CLI and health checks)
pub struct PlayerDaemonConnection {
    reader: Arc<Mutex<BufReader<tokio::io::ReadHalf<UnixStream>>>>,
    writer: Arc<Mutex<tokio::io::WriteHalf<UnixStream>>>,
}

impl PlayerDaemonConnection {
    async fn new(stream: UnixStream) -> Result<Self> {
        let (read_half, write_half) = tokio::io::split(stream);
        Ok(Self {
            reader: Arc::new(Mutex::new(BufReader::new(read_half))),
            writer: Arc::new(Mutex::new(write_half)),
        })
    }

    /// Send a command and wait for response (5-second timeout)
    pub async fn send_command(&mut self, msg: ClientMessage) -> Result<DaemonMessage> {
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            self.send_command_inner(msg),
        )
        .await
        .map_err(|_| anyhow::anyhow!("Daemon request timed out"))?
    }

    async fn send_command_inner(&mut self, msg: ClientMessage) -> Result<DaemonMessage> {
        let json = serde_json::to_string(&msg)?;

        {
            let mut writer = self.writer.lock().await;
            writer.write_all(json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }

        debug!("Sent command: {:?}", msg);

        {
            let mut reader = self.reader.lock().await;
            let mut line = String::new();
            reader.read_line(&mut line).await?;

            if line.is_empty() {
                return Err(anyhow::anyhow!("Daemon connection closed"));
            }

            let response: DaemonMessage = serde_json::from_str(&line)?;
            debug!("Received response: {:?}", response);

            Ok(response)
        }
    }

    pub async fn play(&mut self, station_name: String, url: String) -> Result<()> {
        self.send_command(ClientMessage::Play { station_name, url }).await?;
        Ok(())
    }

    pub async fn pause(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Pause).await?;
        Ok(())
    }

    pub async fn resume(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Resume).await?;
        Ok(())
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Stop).await?;
        Ok(())
    }

    pub async fn set_volume(&mut self, volume: f32) -> Result<()> {
        let vol = volume.max(0.0).min(1.0);
        self.send_command(ClientMessage::SetVolume(vol)).await?;
        Ok(())
    }

    pub async fn reload(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Reload).await?;
        Ok(())
    }

    pub async fn clear_error(&mut self) -> Result<()> {
        self.send_command(ClientMessage::ClearError).await?;
        Ok(())
    }

    pub async fn get_status(&mut self) -> Result<PlayerInfo> {
        match self.send_command(ClientMessage::GetStatus).await? {
            DaemonMessage::State {
                state,
                station_name,
                station_url,
                volume,
                error_message,
            } => Ok(PlayerInfo {
                state: state.into(),
                station_name,
                station_url,
                volume,
                error_message,
            }),
            _ => Err(anyhow::anyhow!("Unexpected response from daemon")),
        }
    }
}
