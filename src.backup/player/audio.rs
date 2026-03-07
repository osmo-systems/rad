use anyhow::{Context, Result};
use rodio::{OutputStream, OutputStreamHandle, Sink, Decoder};
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};
use stream_download::{StreamDownload, Settings};
use stream_download::source::DecodeError;
use stream_download::storage::temp::TempStorageProvider;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PlayerState {
    Stopped,
    Playing,
    Paused,
    Loading,
    Error,
}

#[derive(Debug, Clone)]
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

pub struct AudioPlayer {
    _stream: OutputStream,
    _stream_handle: OutputStreamHandle,
    sink: Arc<Mutex<Option<Sink>>>,
    info: Arc<Mutex<PlayerInfo>>,
    command_tx: mpsc::UnboundedSender<PlayerCommand>,
    current_url: Option<String>,
    current_name: Option<String>,
}

impl AudioPlayer {
    pub fn new() -> Result<(Self, mpsc::UnboundedReceiver<PlayerCommand>)> {
        let (_stream, _stream_handle) = OutputStream::try_default()
            .context("Failed to initialize audio output stream")?;

        let info = Arc::new(Mutex::new(PlayerInfo {
            state: PlayerState::Stopped,
            station_name: String::new(),
            station_url: String::new(),
            volume: 0.5,
            error_message: None,
        }));

        let (command_tx, command_rx) = mpsc::unbounded_channel();

        let player = Self {
            _stream,
            _stream_handle: _stream_handle.clone(),
            sink: Arc::new(Mutex::new(None)),
            info,
            command_tx,
            current_url: None,
            current_name: None,
        };

        Ok((player, command_rx))
    }

    pub fn get_info(&self) -> PlayerInfo {
        self.info.lock().unwrap().clone()
    }

    pub fn clear_error(&mut self) {
        let mut info = self.info.lock().unwrap();
        info.error_message = None;
        tracing::info!("Player error cleared");
    }

    pub fn get_command_sender(&self) -> mpsc::UnboundedSender<PlayerCommand> {
        self.command_tx.clone()
    }

    pub fn play(&mut self, station_name: String, url: String) {
        info!("Starting playback: {} - {}", station_name, url);

        // Stop current playback first
        self.stop();

        // Now update state to loading and set station info
        {
            let mut info = self.info.lock().unwrap();
            info.state = PlayerState::Loading;
            info.station_name = station_name.clone();
            info.station_url = url.clone();
            info.error_message = None;
        }

        // Store current station info
        self.current_url = Some(url.clone());
        self.current_name = Some(station_name.clone());

        // Spawn background task to handle the async loading
        let info_clone = Arc::clone(&self.info);
        let sink_clone = Arc::clone(&self.sink);
        let stream_handle = self._stream_handle.clone();
        
        tokio::spawn(async move {
            info!("Background: Starting to resolve and play URL: {}", url);
            
            // Resolve URL and play
            match Self::resolve_and_play_async(&url, info_clone.clone(), sink_clone, stream_handle).await {
                Ok(()) => {
                    info!("Background: Playback task completed");
                }
                Err(e) => {
                    error!("Background: Failed to play station: {}", e);
                    let mut info = info_clone.lock().unwrap();
                    info.state = PlayerState::Error;
                    info.error_message = Some(format!("{}", e));
                }
            }
        });
    }

    async fn resolve_and_play_async(
        url: &str,
        info: Arc<Mutex<PlayerInfo>>,
        sink_arc: Arc<Mutex<Option<Sink>>>,
        stream_handle: OutputStreamHandle,
    ) -> Result<()> {
        debug!("Resolving URL: {}", url);

        // Check if this is a playlist file
        if url.ends_with(".m3u") || url.ends_with(".pls") {
            debug!("Detected playlist file, parsing...");
            let stream_url = Self::parse_playlist(url).await?;
            return Self::play_stream_async(&stream_url, info, sink_arc, stream_handle).await;
        }

        // Try to play directly
        Self::play_stream_async(url, info, sink_arc, stream_handle).await
    }

    async fn parse_playlist(url: &str) -> Result<String> {
        debug!("Fetching playlist from: {}", url);

        let response = reqwest::get(url)
            .await
            .context("Failed to fetch playlist")?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP error fetching playlist: {}", response.status());
        }

        let content = response.text().await.context("Failed to read playlist")?;

        debug!("Playlist content (first 500 chars): {}", &content[..content.len().min(500)]);

        // Parse M3U
        if url.ends_with(".m3u") || content.starts_with("#EXTM3U") {
            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') && (line.starts_with("http") || line.starts_with("//")) {
                    info!("Found stream URL in M3U: {}", line);
                    return Ok(line.to_string());
                }
            }
        }

        // Parse PLS
        if url.ends_with(".pls") {
            for line in content.lines() {
                if line.starts_with("File") {
                    if let Some(stream_url) = line.split('=').nth(1) {
                        info!("Found stream URL in PLS: {}", stream_url);
                        return Ok(stream_url.trim().to_string());
                    }
                }
            }
        }

        anyhow::bail!("No stream URL found in playlist")
    }

    async fn play_stream_async(
        url: &str,
        info: Arc<Mutex<PlayerInfo>>,
        sink_arc: Arc<Mutex<Option<Sink>>>,
        stream_handle: OutputStreamHandle,
    ) -> Result<()> {
        info!("Playing stream with stream-download: {}", url);

        // Parse URL
        let parsed_url = url.parse()
            .context("Failed to parse stream URL")?;

        // Create stream downloader with temporary storage
        // This handles infinite streaming automatically
        info!("Creating StreamDownload for: {}", url);
        let reader = match StreamDownload::new_http(
            parsed_url,
            TempStorageProvider::new(),
            Settings::default(),
        )
        .await
        {
            Ok(reader) => reader,
            Err(e) => {
                let error_msg = e.decode_error().await;
                error!("Failed to create stream: {}", error_msg);
                anyhow::bail!("Failed to connect to stream: {}", error_msg);
            }
        };

        info!("Stream created successfully, spawning blocking task for decoder");

        // The rest needs to run in a blocking context because rodio's Decoder
        // will perform blocking I/O operations
        let info_clone = Arc::clone(&info);
        let sink_arc_clone = Arc::clone(&sink_arc);
        
        tokio::task::spawn_blocking(move || {
            info!("Blocking task: Creating decoder");
            
            // Wrap in BufReader for better performance
            let buf_reader = BufReader::new(reader);

            // Create decoder - stream-download provides Read + Seek
            let source = match Decoder::new(buf_reader) {
                Ok(s) => {
                    info!("Blocking task: Decoder created successfully");
                    s
                }
                Err(e) => {
                    error!("Blocking task: Failed to create decoder: {}", e);
                    let mut player_info = info_clone.lock().unwrap();
                    player_info.state = PlayerState::Error;
                    player_info.error_message = Some(format!("Failed to decode audio: {}", e));
                    return;
                }
            };

            info!("Blocking task: Creating sink");

            // Create sink
            let sink = match Sink::try_new(&stream_handle) {
                Ok(s) => {
                    info!("Blocking task: Sink created successfully");
                    s
                }
                Err(e) => {
                    error!("Blocking task: Failed to create sink: {}", e);
                    let mut player_info = info_clone.lock().unwrap();
                    player_info.state = PlayerState::Error;
                    player_info.error_message = Some(format!("Failed to create audio sink: {}", e));
                    return;
                }
            };

            // Set volume
            let volume = info_clone.lock().unwrap().volume;
            sink.set_volume(volume);

            info!("Blocking task: Appending source to sink");

            // Play - the stream-download library handles continuous streaming in the background
            sink.append(source);

            // Store the sink so we can control it (pause/resume/stop)
            *sink_arc_clone.lock().unwrap() = Some(sink);

            // Update state to playing
            {
                let mut player_info = info_clone.lock().unwrap();
                player_info.state = PlayerState::Playing;
            }

            info!("Blocking task: Audio playback started successfully");

            // Keep this task alive - the sink needs to stay in scope
            // Check periodically if we should stop
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                
                // Check if sink still exists (not stopped)
                if sink_arc_clone.lock().unwrap().is_none() {
                    info!("Blocking task: Sink was stopped, ending playback task");
                    break;
                }
                
                // Check if sink is empty (finished playing)
                if let Some(ref sink) = *sink_arc_clone.lock().unwrap() {
                    if sink.empty() {
                        info!("Blocking task: Sink is empty, playback finished");
                        break;
                    }
                }
            }
            
            info!("Blocking task: Playback task ended");
        })
        .await
        .context("Blocking task panicked")?;

        Ok(())
    }

    pub fn pause(&mut self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            if !sink.is_paused() {
                info!("Pausing playback");
                sink.pause();
                let mut info = self.info.lock().unwrap();
                info.state = PlayerState::Paused;
            }
        }
    }

    pub fn resume(&mut self) {
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            if sink.is_paused() {
                info!("Resuming playback");
                sink.play();
                let mut info = self.info.lock().unwrap();
                info.state = PlayerState::Playing;
            }
        }
    }

    pub fn stop(&mut self) {
        let mut sink_guard = self.sink.lock().unwrap();
        if let Some(sink) = sink_guard.take() {
            info!("Stopping playback");
            sink.stop();
        }
        drop(sink_guard); // Release lock before modifying other fields
        
        self.current_url = None;
        self.current_name = None;
        let mut info = self.info.lock().unwrap();
        info.state = PlayerState::Stopped;
        info.station_name = String::new();
        info.station_url = String::new();
        info.error_message = None;
    }

    pub fn shutdown(&mut self) {
        // Immediate shutdown - stop playback and clear state
        info!("Shutting down audio player");
        let mut sink_guard = self.sink.lock().unwrap();
        if let Some(sink) = sink_guard.take() {
            // Stop immediately without waiting
            sink.stop();
        }
        drop(sink_guard);
        
        self.current_url = None;
        self.current_name = None;
        // No need to update info state since we're shutting down
    }

    pub fn set_volume(&mut self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
        // Quantize to nearest 1% to avoid floating point precision issues
        let volume = (volume * 100.0).round() / 100.0;
        debug!("Setting volume to: {:.2}", volume);
        
        if let Some(ref sink) = *self.sink.lock().unwrap() {
            sink.set_volume(volume);
        }
        
        let mut info = self.info.lock().unwrap();
        info.volume = volume;
    }

    pub fn reload(&mut self) {
        if let (Some(url), Some(name)) = (self.current_url.clone(), self.current_name.clone()) {
            info!("Reloading station: {}", name);
            self.play(name, url);
        } else {
            warn!("No station to reload");
        }
    }

    #[allow(dead_code)]
    pub fn is_playing(&self) -> bool {
        matches!(self.get_info().state, PlayerState::Playing)
    }

    #[allow(dead_code)]
    pub fn is_stopped(&self) -> bool {
        matches!(self.get_info().state, PlayerState::Stopped)
    }
}
