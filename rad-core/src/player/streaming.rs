use anyhow::{Context, Result};
use rodio::{Decoder, OutputStreamHandle, Sink};
use std::io::BufReader;
use std::sync::{Arc, Mutex};
use stream_download::source::DecodeError;
use stream_download::storage::temp::TempStorageProvider;
use stream_download::{Settings, StreamDownload};
use tracing::{debug, error, info};

use super::types::{PlayerInfo, PlayerState};

pub(super) async fn resolve_and_play_async(
    url: &str,
    info: Arc<Mutex<PlayerInfo>>,
    sink_arc: Arc<Mutex<Option<Sink>>>,
    stream_handle: OutputStreamHandle,
) -> Result<()> {
    debug!("Resolving URL: {}", url);

    if url.ends_with(".m3u") || url.ends_with(".pls") {
        debug!("Detected playlist file, parsing...");
        let stream_url = parse_playlist(url).await?;
        return play_stream_async(&stream_url, info, sink_arc, stream_handle).await;
    }

    play_stream_async(url, info, sink_arc, stream_handle).await
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
            if !line.is_empty()
                && !line.starts_with('#')
                && (line.starts_with("http") || line.starts_with("//"))
            {
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

    let parsed_url = url.parse().context("Failed to parse stream URL")?;

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

    let info_clone = Arc::clone(&info);
    let sink_arc_clone = Arc::clone(&sink_arc);

    tokio::task::spawn_blocking(move || {
        info!("Blocking task: Creating decoder");

        let buf_reader = BufReader::new(reader);

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

        let volume = info_clone.lock().unwrap().volume;
        sink.set_volume(volume);

        info!("Blocking task: Appending source to sink");

        sink.append(source);

        *sink_arc_clone.lock().unwrap() = Some(sink);

        {
            let mut player_info = info_clone.lock().unwrap();
            player_info.state = PlayerState::Playing;
        }

        info!("Blocking task: Audio playback started successfully");

        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));

            if sink_arc_clone.lock().unwrap().is_none() {
                info!("Blocking task: Sink was stopped, ending playback task");
                break;
            }

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
