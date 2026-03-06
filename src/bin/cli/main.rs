//! LazyRadio CLI Application
//! 
//! A command-line interface for browsing and playing radio stations from Radio Browser API.
//! Uses the player daemon for audio playback control.

use anyhow::Result;
use std::io::{self, Write};
use tracing::info;
use tracing_subscriber;

use lazyradio::{
    config::{cleanup_old_logs, get_data_dir},
    search::{parse_query, SearchQuery},
    PlayerDaemonClient, RadioBrowserClient, Station,
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

    // Connect to player daemon
    let daemon_client = PlayerDaemonClient::new()?;
    let mut daemon_conn = match daemon_client.connect().await {
        Ok(conn) => {
            info!("Connected to player daemon");
            conn
        }
        Err(e) => {
            eprintln!("Failed to connect to player daemon: {}", e);
            return Err(e);
        }
    };

    let mut api_client = api_client;

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
                match daemon_conn.get_status().await {
                    Ok(info) => print_player_status(&info),
                    Err(e) => eprintln!("Failed to get player status: {}", e),
                }
            }
            ["play", station_name, url] => {
                match daemon_conn.play(station_name.to_string(), url.to_string()).await {
                    Ok(_) => println!("Playing: {}", station_name),
                    Err(e) => eprintln!("Failed to play station: {}", e),
                }
            }
            ["pause"] => {
                match daemon_conn.pause().await {
                    Ok(_) => println!("Paused"),
                    Err(e) => eprintln!("Failed to pause: {}", e),
                }
            }
            ["resume"] => {
                match daemon_conn.resume().await {
                    Ok(_) => println!("Resumed"),
                    Err(e) => eprintln!("Failed to resume: {}", e),
                }
            }
            ["stop"] => {
                match daemon_conn.stop().await {
                    Ok(_) => println!("Stopped"),
                    Err(e) => eprintln!("Failed to stop: {}", e),
                }
            }
            ["volume", vol_str] => {
                if let Ok(vol) = vol_str.parse::<f32>() {
                    let clamped_vol = vol.max(0.0).min(1.0);
                    match daemon_conn.set_volume(clamped_vol).await {
                        Ok(_) => println!("Volume set to {:.0}%", clamped_vol * 100.0),
                        Err(e) => eprintln!("Failed to set volume: {}", e),
                    }
                } else {
                    eprintln!("Invalid volume. Please use a value between 0 and 1");
                }
            }
            ["search", query] => {
                match parse_query(query) {
                    Ok(search_query) => {
                        match search_stations(&mut api_client, &search_query).await {
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
                match get_popular_stations(&mut api_client, limit).await {
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

        // No need to handle command responses since we're using async daemon communication
    }

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
            lazyradio::PlayerState::Playing => "Playing",
            lazyradio::PlayerState::Paused => "Paused",
            lazyradio::PlayerState::Stopped => "Stopped",
            lazyradio::PlayerState::Loading => "Loading",
            lazyradio::PlayerState::Error => "Error",
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
    client: &mut RadioBrowserClient,
    query: &SearchQuery,
) -> Result<Vec<Station>> {
    // Convert SearchQuery to a search string and call the API
    let mut search_parts = Vec::new();

    if let Some(name) = &query.name {
        search_parts.push(name.as_str());
    }

    let search_str = search_parts.join(" ");
    
    // Use search_stations from the API client
    // This is a simplified approach - just search by name for now
    if search_str.is_empty() {
        client.get_popular_stations(50).await
    } else {
        client.search_stations(&search_str, 50).await
    }
}

async fn get_popular_stations(client: &mut RadioBrowserClient, limit: usize) -> Result<Vec<Station>> {
    // Fetch popular stations from the API
    client.get_popular_stations(limit).await
}
