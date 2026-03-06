//! LazyRadio CLI Application
//! 
//! A command-line interface for controlling the radio player daemon.
//! Supports one-liner commands like: radiocli pause, radiocli volume 50, etc.

use anyhow::Result;
use std::env;
use tracing::info;

use lazyradio::{
    config::{cleanup_old_logs, get_data_dir, Config},
    PlayerDaemonClient,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "lazyradio-cli.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    info!("LazyRadio CLI starting with args: {:?}", env::args().collect::<Vec<_>>());

    // Clean up old log files
    if let Err(e) = cleanup_old_logs(&data_dir, 7) {
        tracing::warn!("Failed to clean up old logs: {}", e);
    }

    // Get command line arguments
    let args: Vec<String> = env::args().collect();

    // Connect to player daemon (auto-starts if needed)
    let daemon_client = PlayerDaemonClient::new()?;
    let mut daemon_conn = match daemon_client.connect().await {
        Ok(conn) => {
            info!("Connected to player daemon");
            conn
        }
        Err(e) => {
            eprintln!("Error: Failed to connect to player daemon: {}", e);
            return Err(e);
        }
    };

    // Parse and execute commands
    match args.get(1).map(|s| s.as_str()) {
        None | Some("status") => {
            // Show status or resume last station if daemon just started
            match daemon_conn.get_status().await {
                Ok(info) => {
                    // If stopped and no station is playing, try to resume last station
                    if info.state == lazyradio::PlayerState::Stopped 
                        && info.station_name.is_empty() 
                        && info.station_url.is_empty() {
                        
                        // Load config to get last station
                        let config = Config::load(&data_dir)?;
                        if let (Some(name), Some(url)) = (config.last_station_name, config.last_station_url) {
                            info!("Resuming last station: {} ({})", name, url);
                            if let Err(e) = daemon_conn.play(name.clone(), url.clone()).await {
                                eprintln!("Error: Failed to resume last station: {}", e);
                                // Continue and show status anyway
                            }
                        }
                    }
                    
                    // Show current status
                    println!("\nPlayer Status:");
                    println!("  State:       {}", match info.state {
                        lazyradio::PlayerState::Playing => "Playing",
                        lazyradio::PlayerState::Paused => "Paused",
                        lazyradio::PlayerState::Stopped => "Stopped",
                        lazyradio::PlayerState::Loading => "Loading",
                        lazyradio::PlayerState::Error => "Error",
                    });
                    println!("  Station:     {}", if info.station_name.is_empty() { "None" } else { &info.station_name });
                    println!("  Volume:      {:.0}%", info.volume * 100.0);
                    if let Some(err) = &info.error_message {
                        println!("  Error:       {}", err);
                    }
                    println!();
                }
                Err(e) => {
                    eprintln!("Error: Failed to get player status: {}", e);
                    return Err(e);
                }
            }
        }
        Some("play") => {
            // Resume playback
            if let Err(e) = daemon_conn.resume().await {
                eprintln!("Error: Failed to resume playback: {}", e);
                return Err(e);
            }
            println!("Resumed");
        }
        Some("play-url") => {
            // Play a station by name and URL: radiocli play-url "Station Name" "http://url"
            match (args.get(2), args.get(3)) {
                (Some(name), Some(url)) => {
                    // Save station as last played
                    let mut config = Config::load(&data_dir)?;
                    config.update_session_state(config.default_volume, Some(name.clone()), Some(url.clone()));
                    config.save(&data_dir)?;
                    
                    // Play the station
                    if let Err(e) = daemon_conn.play(name.clone(), url.clone()).await {
                        eprintln!("Error: Failed to play station: {}", e);
                        return Err(e);
                    }
                    println!("Playing: {}", name);
                }
                _ => {
                    eprintln!("Error: Usage: radiocli play-url <name> <url>");
                    return Err(anyhow::anyhow!("Missing arguments for play-url command"));
                }
            }
        }
        Some("pause") => {
            // Pause playback
            if let Err(e) = daemon_conn.pause().await {
                eprintln!("Error: Failed to pause playback: {}", e);
                return Err(e);
            }
            println!("Paused");
        }
        Some("resume") => {
            // Resume playback
            if let Err(e) = daemon_conn.resume().await {
                eprintln!("Error: Failed to resume playback: {}", e);
                return Err(e);
            }
            println!("Resumed");
        }
        Some("stop") => {
            // Stop playback
            if let Err(e) = daemon_conn.stop().await {
                eprintln!("Error: Failed to stop playback: {}", e);
                return Err(e);
            }
            println!("Stopped");
        }
        Some("volume") => {
            match args.get(2).map(|s| s.as_str()) {
                Some("--up") => {
                    // Increase volume by specified amount (default 10%)
                    let amount = args.get(3)
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(10.0) / 100.0;
                    
                    match daemon_conn.get_status().await {
                        Ok(info) => {
                            let new_vol = (info.volume + amount).min(1.0);
                            if let Err(e) = daemon_conn.set_volume(new_vol).await {
                                eprintln!("Error: Failed to set volume: {}", e);
                                return Err(e);
                            }
                            println!("Volume: {:.0}%", new_vol * 100.0);
                        }
                        Err(e) => {
                            eprintln!("Error: Failed to get current volume: {}", e);
                            return Err(e);
                        }
                    }
                }
                Some("--down") => {
                    // Decrease volume by specified amount (default 10%)
                    let amount = args.get(3)
                        .and_then(|s| s.parse::<f32>().ok())
                        .unwrap_or(10.0) / 100.0;
                    
                    match daemon_conn.get_status().await {
                        Ok(info) => {
                            let new_vol = (info.volume - amount).max(0.0);
                            if let Err(e) = daemon_conn.set_volume(new_vol).await {
                                eprintln!("Error: Failed to set volume: {}", e);
                                return Err(e);
                            }
                            println!("Volume: {:.0}%", new_vol * 100.0);
                        }
                        Err(e) => {
                            eprintln!("Error: Failed to get current volume: {}", e);
                            return Err(e);
                        }
                    }
                }
                Some(vol_str) => {
                    // Set volume to specified percentage (0-100)
                    if let Ok(vol_percent) = vol_str.parse::<f32>() {
                        let vol = (vol_percent / 100.0).max(0.0).min(1.0);
                        if let Err(e) = daemon_conn.set_volume(vol).await {
                            eprintln!("Error: Failed to set volume: {}", e);
                            return Err(e);
                        }
                        println!("Volume: {:.0}%", vol * 100.0);
                    } else {
                        eprintln!("Error: Invalid volume. Use 0-100 for percentage, --up <amount>, or --down <amount>");
                        return Err(anyhow::anyhow!("Invalid volume argument"));
                    }
                }
                None => {
                    // Show current volume
                    match daemon_conn.get_status().await {
                        Ok(info) => println!("Volume: {:.0}%", info.volume * 100.0),
                        Err(e) => {
                            eprintln!("Error: Failed to get volume: {}", e);
                            return Err(e);
                        }
                    }
                }
            }
        }
        Some("quit") | Some("exit") => {
            println!("Exiting LazyRadio CLI");
            return Ok(());
        }
        Some("help") | Some("-h") | Some("--help") => {
            print_help();
        }
        Some(cmd) => {
            eprintln!("Error: Unknown command '{}'. Use 'radiocli help' for available commands.", cmd);
            return Err(anyhow::anyhow!("Unknown command: {}", cmd));
        }
    }

    info!("LazyRadio CLI completed successfully");
    Ok(())
}

fn print_help() {
    println!("\n╭────────────────────────────────────────────────────────────┐");
    println!("│ LazyRadio CLI - Radio Player Control                       │");
    println!("├────────────────────────────────────────────────────────────┤");
    println!("│ Usage: radiocli <command> [options]                        │");
    println!("│                                                            │");
    println!("│ Commands:                                                  │");
    println!("│   status              - Show current player status         │");
    println!("│   play                - Resume playback                    │");
    println!("│   play-url <n> <url>  - Play station and save as last      │");
    println!("│   pause               - Pause playback                     │");
    println!("│   resume              - Resume playback                    │");
    println!("│   stop                - Stop playback                      │");
    println!("│   volume <0-100>      - Set volume (0-100%)                │");
    println!("│   volume --up [amt]   - Increase volume (default 10%)      │");
    println!("│   volume --down [amt] - Decrease volume (default 10%)      │");
    println!("│   quit/exit           - Exit CLI                           │");
    println!("│   help                - Show this help message             │");
    println!("╰────────────────────────────────────────────────────────────╯\n");
}
