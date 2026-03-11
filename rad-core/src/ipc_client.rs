//! IPC Client for Player Daemon
//!
//! Provides a client interface for TUI/CLI applications to communicate with
//! the headless player daemon via Unix socket.

use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, info};

use crate::config::get_data_dir;
use crate::ipc::{ClientMessage, DaemonMessage};
use crate::player::PlayerInfo;

const DAEMON_SOCKET: &str = ".radm-player.sock";
const DAEMON_BINARY: &str = "_rad-daemon";

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

    /// Ensure daemon is running and connect to it
    pub async fn connect(&self) -> Result<PlayerDaemonConnection> {
        // Try to connect to existing daemon
        if let Ok(stream) = UnixStream::connect(&self.socket_path).await {
            info!("Connected to existing player daemon");
            let mut conn = PlayerDaemonConnection::new(stream).await?;

            // Health-check: verify the daemon responds within 3 seconds.
            // An old single-threaded daemon will accept the socket connection at
            // the OS level but never read from it while another client is active,
            // so the health check times out and we kill + restart it.
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

            // Daemon is unresponsive — kill it and fall through to a fresh start
            self.kill_daemon();
        }

        // Daemon not running (or was just killed), try to start it
        info!("Player daemon not running, attempting to start it...");

        // Remove stale socket file if it exists
        if self.socket_path.exists() {
            info!("Removing stale socket file: {}", self.socket_path.display());
            if let Err(e) = std::fs::remove_file(&self.socket_path) {
                tracing::warn!("Failed to remove stale socket file: {}", e);
            }
        }

        self.start_daemon().await?;

        // Wait for daemon to be ready
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

    /// Kill the running daemon process and remove its socket.
    fn kill_daemon(&self) {
        info!("Killing unresponsive daemon");
        // pkill matches on process name; -x requires an exact name match
        let _ = std::process::Command::new("pkill")
            .arg("-x")
            .arg(DAEMON_BINARY)
            .status();
        if self.socket_path.exists() {
            let _ = std::fs::remove_file(&self.socket_path);
        }
        // Give the OS a moment to reap the process and release the socket
        std::thread::sleep(std::time::Duration::from_millis(300));
    }

    /// Start the daemon process
    async fn start_daemon(&self) -> Result<()> {
        // Try to find the daemon binary in the same directory as the current executable
        let current_exe = std::env::current_exe()?;
        let exe_dir = current_exe
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Failed to determine executable directory"))?;

        let daemon_path = exe_dir.join(DAEMON_BINARY);

        info!("Current executable: {}", current_exe.display());
        info!("Executable directory: {}", exe_dir.display());
        info!("Attempting to start daemon from: {}", daemon_path.display());

        // First try the full path
        if daemon_path.exists() {
            info!("Found daemon at: {}", daemon_path.display());
            Command::new(&daemon_path).spawn().context(format!(
                "Failed to start player daemon from {}",
                daemon_path.display()
            ))?;
            info!("Spawned player daemon from: {}", daemon_path.display());
            return Ok(());
        }

        info!("Daemon not found at {}, trying PATH", daemon_path.display());

        // Fallback: try from PATH
        Command::new(DAEMON_BINARY)
            .spawn()
            .context("Failed to start player daemon. Make sure 'rad-daemon' is installed and in PATH or in the same directory as this executable")?;

        info!("Spawned player daemon process from PATH");
        Ok(())
    }

    /// Check if daemon is running
    pub async fn is_running(&self) -> bool {
        UnixStream::connect(&self.socket_path).await.is_ok()
    }
}

/// Active connection to the player daemon
pub struct PlayerDaemonConnection {
    reader: Arc<Mutex<BufReader<tokio::io::ReadHalf<UnixStream>>>>,
    writer: Arc<Mutex<tokio::io::WriteHalf<UnixStream>>>,
}

impl PlayerDaemonConnection {
    /// Create a new connection from a UnixStream
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

        // Write command
        {
            let mut writer = self.writer.lock().await;
            writer.write_all(json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }

        debug!("Sent command: {:?}", msg);

        // Read response
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

    /// Play a station
    pub async fn play(&mut self, station_name: String, url: String) -> Result<()> {
        let msg = ClientMessage::Play { station_name, url };
        self.send_command(msg).await?;
        Ok(())
    }

    /// Pause playback
    pub async fn pause(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Pause).await?;
        Ok(())
    }

    /// Resume playback
    pub async fn resume(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Resume).await?;
        Ok(())
    }

    /// Stop playback
    pub async fn stop(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Stop).await?;
        Ok(())
    }

    /// Set volume (0.0 to 1.0)
    pub async fn set_volume(&mut self, volume: f32) -> Result<()> {
        let vol = volume.max(0.0).min(1.0);
        self.send_command(ClientMessage::SetVolume(vol)).await?;
        Ok(())
    }

    /// Reload current station
    pub async fn reload(&mut self) -> Result<()> {
        self.send_command(ClientMessage::Reload).await?;
        Ok(())
    }

    /// Clear error state
    pub async fn clear_error(&mut self) -> Result<()> {
        self.send_command(ClientMessage::ClearError).await?;
        Ok(())
    }

    /// Get current player status
    pub async fn get_status(&mut self) -> Result<PlayerInfo> {
        match self.send_command(ClientMessage::GetStatus).await? {
            DaemonMessage::Status {
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
