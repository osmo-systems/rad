//! LazyRadio CLI Application
//! 
//! A command-line interface for browsing and playing radio stations from Radio Browser API.

use anyhow::Result;
use std::io::{self, Write};
use tracing::info;
use tracing_subscriber;

use lazyradio::{
    config::{cleanup_old_logs, get_data_dir},
    player::{AudioPlayer, PlayerCommand, PlayerState},
    search::{parse_query, SearchQuery},
    RadioBrowserClient, Station,
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

    info!("Starting LazyRadio CLI");

    // Clean up old log files
    if let Err(e) = cleanup_old_logs(&data_dir, 7) {
        tracing::warn!("Failed to clean up old logs: {}", e);
    }

    // Initialize API client
    let api_client = match RadioBrowserClient::new().await {
        Ok(client) => {
            info!("API client initialized");
            client
        }
        Err(e) => {
            eprintln!("Failed to initialize Radio Browser API client: {}", e);
            eprintln!("Please check your internet connection and DNS configuration.");
            return Err(e);
        }
    };

    // Initialize audio player
    let (mut audio_player, mut player_cmd_rx) = match AudioPlayer::new() {
        Ok((player, rx)) => {
            info!("Audio player initialized");
            (player, rx)
        }
        Err(e) => {
            eprintln!("Failed to initialize audio device: {}", e);
            eprintln!("Please check that your audio output is properly configured.");
            return Err(e);
        }
    };

    let player_cmd_tx = audio_player.get_command_sender();

    // Show welcome message
    println!("\n╭─────────────────────────────────────────────────────────────╮");
    println!("│ LazyRadio CLI - Radio Browser Terminal Client                │");
    println!("│ Type 'help' for available commands                            │");
    println!("╰─────────────────────────────────────────────────────────────╯\n");

    // Command loop
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut input = String::new();

    loop {
        print!("lazy-radio> ");
        stdout.flush()?;
        input.clear();
        stdin.read_line(&mut input)?;

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        match parts.as_slice() {
            ["help"] => {
                print_help();
            }
            ["exit"] | ["quit"] => {
                println!("Exiting LazyRadio CLI. Goodbye!");
                break;
            }
            ["status"] => {
                let info = audio_player.get_info();
                print_player_status(&info);
            }
            ["play", station_name, url] => {
                player_cmd_tx.send(PlayerCommand::Play(station_name.to_string(), url.to_string()))?;
                println!("Playing: {}", station_name);
            }
            ["pause"] => {
                player_cmd_tx.send(PlayerCommand::Pause)?;
                println!("Paused");
            }
            ["resume"] => {
                player_cmd_tx.send(PlayerCommand::Resume)?;
                println!("Resumed");
            }
            ["stop"] => {
                player_cmd_tx.send(PlayerCommand::Stop)?;
                println!("Stopped");
            }
            ["volume", vol_str] => {
                if let Ok(vol) = vol_str.parse::<f32>() {
                    let clamped_vol = vol.max(0.0).min(1.0);
                    player_cmd_tx.send(PlayerCommand::SetVolume(clamped_vol))?;
                    println!("Volume set to {:.0}%", clamped_vol * 100.0);
                } else {
                    eprintln!("Invalid volume. Please use a value between 0 and 1");
                }
            }
            ["search", query] => {
                match parse_query(query) {
                    Ok(search_query) => {
                        match search_stations(&api_client, &search_query).await {
                            Ok(stations) => {
                                println!("\nFound {} stations:\n", stations.len());
                         for (i, station) in stations.iter().enumerate().take(10) {
                            println!(
                                "{}: {} - {} [{}]",
                                i + 1,
                                station.name,
                                if station.country.is_empty() { "Unknown" } else { &station.country },
                                if station.language.is_empty() { "Unknown" } else { &station.language }
                            );
                        }
                                if stations.len() > 10 {
                                    println!("\n... and {} more stations", stations.len() - 10);
                                }
                                println!();
                            }
                            Err(e) => {
                                eprintln!("Search failed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Invalid query: {}", e);
                    }
                }
            }
            ["popular", limit_str] => {
                let limit = limit_str.parse::<usize>().unwrap_or(10);
                match get_popular_stations(&api_client, limit).await {
                    Ok(stations) => {
                        println!("\nTop {} popular stations:\n", stations.len());
                        for (i, station) in stations.iter().enumerate() {
                            println!(
                                "{}: {} - {} (votes: {})",
                                i + 1,
                                station.name,
                                if station.country.is_empty() { "Unknown" } else { &station.country },
                                station.votes
                            );
                        }
                        println!();
                    }
                    Err(e) => {
                        eprintln!("Failed to fetch popular stations: {}", e);
                    }
                }
            }
            _ => {
                eprintln!("Unknown command. Type 'help' for available commands.");
            }
        }

        // Handle player command responses
        while let Ok(cmd) = player_cmd_rx.try_recv() {
            info!("Processing command from player");
        }
    }

    audio_player.shutdown();
    info!("LazyRadio CLI shutting down");

    Ok(())
}

fn print_help() {
    println!("\n╭─────────────────────────────────────────────────────────────╮");
    println!("│ Available Commands                                            │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!("│ help                        - Show this help message          │");
    println!("│ exit/quit                   - Exit the application            │");
    println!("│ status                      - Show current player status      │");
    println!("│ play <name> <url>           - Play a station                  │");
    println!("│ pause                       - Pause playback                  │");
    println!("│ resume                      - Resume playback                 │");
    println!("│ stop                        - Stop playback                   │");
    println!("│ volume <0-1>                - Set volume (0.0 to 1.0)        │");
    println!("│ search <query>              - Search for stations             │");
    println!("│ popular [limit]             - Show popular stations (def: 10) │");
    println!("╰─────────────────────────────────────────────────────────────╯\n");
}

fn print_player_status(info: &lazyradio::PlayerInfo) {
    println!("\n╭─────────────────────────────────────────────────────────────╮");
    println!("│ Player Status                                                 │");
    println!("├─────────────────────────────────────────────────────────────┤");
    println!(
        "│ State:       {}",
        match info.state {
            PlayerState::Playing => "Playing",
            PlayerState::Paused => "Paused",
            PlayerState::Stopped => "Stopped",
            PlayerState::Loading => "Loading",
            PlayerState::Error => "Error",
        }
    );
    println!("│ Station:     {}", info.station_name.as_str());
    println!("│ Volume:      {:.0}%", info.volume * 100.0);
    if let Some(err) = &info.error_message {
        println!("│ Error:       {}", err);
    }
    println!("╰─────────────────────────────────────────────────────────────╯\n");
}

async fn search_stations(
    client: &RadioBrowserClient,
    query: &SearchQuery,
) -> Result<Vec<Station>> {
    // Format the query as a search string and call the API
    // This is a simplified implementation
    let mut search_parts = Vec::new();

    if let Some(name) = &query.name {
        search_parts.push(format!("name:{}", name));
    }
    if let Some(country) = &query.country {
        search_parts.push(format!("country:{}", country));
    }
    if let Some(language) = &query.language {
        search_parts.push(format!("language:{}", language));
    }

    let search_str = search_parts.join(" ");
    println!("Searching for: {}", if search_str.is_empty() { "all stations".to_string() } else { search_str });
    Ok(Vec::new())
}

async fn get_popular_stations(client: &RadioBrowserClient, limit: usize) -> Result<Vec<Station>> {
    // This would call the API to fetch popular stations
    // For now, return empty - in a real implementation, this would call the API
    Ok(Vec::new())
}
