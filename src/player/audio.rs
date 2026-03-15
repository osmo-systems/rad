use anyhow::{Context, Result};
use rodio::{OutputStream, OutputStreamHandle, Sink};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::streaming::resolve_and_play_async;
use super::types::{PlayerCommand, PlayerInfo, PlayerState};

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

        self.stop();

        {
            let mut info = self.info.lock().unwrap();
            info.state = PlayerState::Loading;
            info.station_name = station_name.clone();
            info.station_url = url.clone();
            info.error_message = None;
        }

        self.current_url = Some(url.clone());
        self.current_name = Some(station_name.clone());

        let info_clone = Arc::clone(&self.info);
        let sink_clone = Arc::clone(&self.sink);
        let stream_handle = self._stream_handle.clone();

        tokio::spawn(async move {
            info!("Background: Starting to resolve and play URL: {}", url);

            match resolve_and_play_async(&url, info_clone.clone(), sink_clone, stream_handle).await {
                Ok(()) => {
                    info!("Background: Playback task completed");
                }
                Err(e) => {
                    tracing::error!("Background: Failed to play station: {}", e);
                    let mut info = info_clone.lock().unwrap();
                    info.state = PlayerState::Error;
                    info.error_message = Some(format!("{}", e));
                }
            }
        });
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
        drop(sink_guard);

        self.current_url = None;
        self.current_name = None;
        let mut info = self.info.lock().unwrap();
        info.state = PlayerState::Stopped;
        info.station_name = String::new();
        info.station_url = String::new();
        info.error_message = None;
    }

    pub fn shutdown(&mut self) {
        info!("Shutting down audio player");
        let mut sink_guard = self.sink.lock().unwrap();
        if let Some(sink) = sink_guard.take() {
            sink.stop();
        }
        drop(sink_guard);

        self.current_url = None;
        self.current_name = None;
    }

    pub fn set_volume(&mut self, volume: f32) {
        let volume = volume.clamp(0.0, 1.0);
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
