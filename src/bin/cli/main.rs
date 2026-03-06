//! LazyRadio CLI Application
//! 
//! A command-line interface for controlling the radio player daemon.
//! Supports one-liner commands like: radiocli pause, radiocli volume 50, etc.

mod search;

use anyhow::Result;
use std::env;
use tracing::info;
use crossterm::terminal;
use std::io::Write;

use lazyradio::{
    config::{cleanup_old_logs, get_data_dir, Config},
    api::RadioBrowserClient,
    PlayerDaemonClient,
};

use search::{parse_search_args, InteractiveSearch};

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
        Some("search") => {
            // Get terminal width for formatting
            let (terminal_width, _) = terminal::size().unwrap_or((120, 30));

            // Check if interactive mode (no args) or direct search (args provided)
            if args.len() <= 2 {
                // Interactive mode: sequential filter prompts
                run_interactive_search(&daemon_conn, &data_dir).await?;
            } else {
                // Direct search mode: parse args and fetch results
                run_direct_search(&args, terminal_width).await?;
            }
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

/// Run direct search with provided arguments
async fn run_direct_search(args: &[String], terminal_width: u16) -> Result<()> {
    let query = parse_search_args(args);
    
    print!("Fetching");
    std::io::stdout().flush()?;
    
    // Create API client and search
    let mut api_client = RadioBrowserClient::new().await?;
    let results = api_client.advanced_search(&query).await?;
    
    println!(" ✓");
    
    if results.is_empty() {
        println!("No stations found");
        return Ok(());
    }
    
    // Load favorites for display
    let data_dir = get_data_dir()?;
    let favorites = lazyradio::FavoritesManager::new(&data_dir)?;
    
    // Display results
    println!("{}", search::format_station_list(
        &results,
        0,
        &favorites,
        terminal_width as usize,
        query.offset,
    ));
    
    // Show pagination info
    let total_shown = query.offset + results.len();
    println!("\n(Showing results {}-{})", query.offset + 1, total_shown);
    
    if results.len() >= query.limit {
        println!("Use --skip {} to fetch next page", query.offset + query.limit);
    }
    
    Ok(())
}

/// Run interactive search mode with sequential filter prompts
async fn run_interactive_search(_daemon_conn: &lazyradio::PlayerDaemonConnection, data_dir: &std::path::PathBuf) -> Result<()> {
    use std::io::Write;
    
    let mut query = lazyradio::search::SearchQuery::default();
    let (terminal_width, _) = terminal::size().unwrap_or((120, 30));
    
    println!("\n=== Interactive Search ===\n");
    
    // Simple filter input
    print!("Name (press Enter to skip): ");
    std::io::stdout().flush()?;
    let mut name_input = String::new();
    std::io::stdin().read_line(&mut name_input)?;
    let name_input = name_input.trim();
    if !name_input.is_empty() {
        query.name = Some(name_input.to_string());
    }
    
    print!("Country (press Enter to skip): ");
    std::io::stdout().flush()?;
    let mut country_input = String::new();
    std::io::stdin().read_line(&mut country_input)?;
    let country_input = country_input.trim();
    if !country_input.is_empty() {
        query.country = Some(country_input.to_string());
    }
    
    print!("Language (press Enter to skip): ");
    std::io::stdout().flush()?;
    let mut language_input = String::new();
    std::io::stdin().read_line(&mut language_input)?;
    let language_input = language_input.trim();
    if !language_input.is_empty() {
        query.language = Some(language_input.to_string());
    }
    
    // Create interactive search session and run
    let mut interactive = InteractiveSearch::new().await?;
    
    match interactive.run(query, terminal_width).await? {
        Some((station_name, station_url)) => {
            // Play the selected station
            // Create a new connection (daemon_conn is immutable reference)
            let daemon_client = lazyradio::PlayerDaemonClient::new()?;
            let mut new_conn = daemon_client.connect().await?;
            
            if let Err(e) = new_conn.play(station_name.clone(), station_url.clone()).await {
                eprintln!("Error: Failed to play station: {}", e);
                return Err(e);
            }
            
            // Save as last played
            let mut config = Config::load(data_dir)?;
            config.update_session_state(config.default_volume, Some(station_name.clone()), Some(station_url));
            let _ = config.save(data_dir);
            
            println!("Playing: {}", station_name);
        }
        None => {
            println!("Search cancelled");
        }
    }
    
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
    println!("│ Search:                                                    │");
    println!("│   search              - Interactive mode with prompts      │");
    println!("│   search <query>      - Direct search (e.g., jazz)         │");
    println!("│   search --country X  - Filter by country/language/codec   │");
    println!("│   search --limit 20   - Set result limit (default: 100)    │");
    println!("│   search --skip N     - Paginate results (N results)       │");
    println!("│ Other:                                                     │");
    println!("│   quit/exit           - Exit CLI                           │");
    println!("│   help                - Show this help message             │");
    println!("╰────────────────────────────────────────────────────────────╯\n");
}
