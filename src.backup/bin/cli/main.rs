//! Radm CLI Application
//! 
//! A command-line interface for controlling the radio player daemon.
//! Supports one-liner commands like: radc pause, radc volume 50, etc.

mod search;

use anyhow::Result;
use std::env;
use tracing::info;

use radm::{
    config::{cleanup_old_logs, get_data_dir, Config},
    api::RadioBrowserClient,
    PlayerDaemonClient,
};

use search::{parse_search_args, run_interactive_search_with_select, SearchAction};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "radc.log");
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

    // Parse and execute commands
    // Note: Some commands need daemon connection, some don't
    match args.get(1).map(|s| s.as_str()) {
        Some("find") => {
            // Find command - no initial connection needed
            if args.len() <= 2 {
                // Interactive mode: sequential filter prompts
                run_interactive_search(&data_dir).await?;
            } else {
                // Direct search mode: parse args and fetch results
                run_direct_search(&args).await?;
            }
        }
        Some("help") | Some("-h") | Some("--help") => {
            print_help();
        }
        Some("quit") | Some("exit") => {
            println!("Exiting LazyRadio CLI");
            return Ok(());
        }
        _ => {
            // All other commands need daemon connection
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
            
            match args.get(1).map(|s| s.as_str()) {
        None | Some("info") => {
            // Show current player status
            match daemon_conn.get_status().await {
                Ok(info) => {
                    println!("\nPlayer Status:");
                    println!("  State:       {}", match info.state {
                        radm::PlayerState::Playing => "Playing",
                        radm::PlayerState::Paused => "Paused",
                        radm::PlayerState::Stopped => "Stopped",
                        radm::PlayerState::Loading => "Loading",
                        radm::PlayerState::Error => "Error",
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
        Some("pause") => {
            // Pause playback
            if let Err(e) = daemon_conn.pause().await {
                eprintln!("Error: Failed to pause playback: {}", e);
                return Err(e);
            }
            println!("Paused");
        }
        Some("start") => {
            // Start playback - either resume paused or play last station
            match daemon_conn.get_status().await {
                Ok(info) => {
                    if info.state == radm::PlayerState::Paused {
                        // Resume paused playback
                        if let Err(e) = daemon_conn.resume().await {
                            eprintln!("Error: Failed to resume playback: {}", e);
                            return Err(e);
                        }
                        println!("Resumed");
                    } else if info.state == radm::PlayerState::Stopped {
                        // Try to play last station
                        let config = Config::load(&data_dir)?;
                        if let (Some(name), Some(url)) = (config.last_station_name, config.last_station_url) {
                            if let Err(e) = daemon_conn.play(name.clone(), url.clone()).await {
                                eprintln!("Error: Failed to play last station: {}", e);
                                return Err(e);
                            }
                            println!("Playing: {}", name);
                        } else {
                            println!("No station to play - use 'radc find' to search for stations");
                        }
                    } else {
                        println!("Already playing");
                    }
                }
                Err(e) => {
                    eprintln!("Error: Failed to get player status: {}", e);
                    return Err(e);
                }
            }
        }
        Some("zap") => {
            // Stop playback (kill/zap)
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
                    let amount = match args.get(3) {
                        Some(s) => match s.parse::<f32>() {
                            Ok(val) => val / 100.0,
                            Err(_) => {
                                eprintln!("Warning: Invalid amount '{}', using default 10%", s);
                                0.1
                            }
                        },
                        None => 0.1,
                    };
                    
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
                    let amount = match args.get(3) {
                        Some(s) => match s.parse::<f32>() {
                            Ok(val) => val / 100.0,
                            Err(_) => {
                                eprintln!("Warning: Invalid amount '{}', using default 10%", s);
                                0.1
                            }
                        },
                        None => 0.1,
                    };
                    
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
                Some(cmd) => {
                    eprintln!("Error: Unknown command '{}'. Use 'radc help' for available commands.", cmd);
                    return Err(anyhow::anyhow!("Unknown command: {}", cmd));
                }
            }
            // Explicitly drop the daemon connection before exiting
            drop(daemon_conn);
        }
    }

    info!("LazyRadio CLI completed successfully");
    Ok(())
}

/// Run direct search with provided arguments
async fn run_direct_search(args: &[String]) -> Result<()> {
    let mut query = parse_search_args(args);
    
    // Pagination loop
    loop {
        // Create API client and search with proper error handling for spinner
        let spinner = cliclack::spinner();
        spinner.start("Fetching stations...");
        
        let results = match async {
            let mut api_client = RadioBrowserClient::new().await?;
            api_client.advanced_search(&query).await
        }.await {
            Ok(results) => {
                spinner.stop("Stations loaded");
                // Give terminal a moment to settle after spinner stops
                std::thread::sleep(std::time::Duration::from_millis(100));
                results
            }
            Err(e) => {
                spinner.error("Failed to fetch stations");
                return Err(e);
            }
        };
        
        if results.is_empty() && query.offset == 0 {
            println!("No stations found");
            return Ok(());
        }
        
        // Use cliclack for interactive selection with pagination
        match run_interactive_search_with_select(results, &query).await? {
            Some(SearchAction::Play(station_name, station_url)) => {
                // Show spinner while loading station
                let spinner = cliclack::spinner();
                spinner.start(format!("Loading station: {}", station_name));
                
                // Play the selected station
                let daemon_client = radm::PlayerDaemonClient::new()?;
                let mut conn = daemon_client.connect().await?;
                
                if let Err(e) = conn.play(station_name.clone(), station_url.clone()).await {
                    spinner.error("Failed to play station");
                    eprintln!("Error: Failed to play station: {}", e);
                    return Err(e);
                }
                
                // Wait until the station is actually playing
                let mut attempts = 0;
                let max_attempts = 50; // 10 seconds max (50 * 200ms)
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    
                    match conn.get_status().await {
                        Ok(status) => {
                            use radm::player::PlayerState;
                            match status.state {
                                PlayerState::Playing => {
                                    // Stream is loaded and playing!
                                    break;
                                }
                                PlayerState::Error => {
                                    spinner.error("Failed to load station");
                                    if let Some(err_msg) = status.error_message {
                                        eprintln!("Error: {}", err_msg);
                                    }
                                    return Err(anyhow::anyhow!("Station failed to load"));
                                }
                                PlayerState::Loading => {
                                    // Still loading, continue waiting
                                }
                                _ => {
                                    // Unexpected state, but continue waiting
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to get status while waiting: {}", e);
                        }
                    }
                    
                    attempts += 1;
                    if attempts >= max_attempts {
                        spinner.error("Timed out waiting for station to load");
                        return Err(anyhow::anyhow!("Station took too long to load"));
                    }
                }
                
                // Explicitly drop the connection before saving config
                drop(conn);
                
                // Save as last played
                let data_dir = get_data_dir()?;
                let mut config = radm::config::Config::load(&data_dir)?;
                config.update_session_state(config.default_volume, Some(station_name.clone()), Some(station_url));
                if let Err(e) = config.save(&data_dir) {
                    tracing::warn!("Failed to save config: {}", e);
                }
                
                // Show final message with spinner stop
                spinner.stop(format!("Playing {}", station_name));
                println!(); // Empty line at the end
                return Ok(());
            }
            Some(SearchAction::NextPage) => {
                // Go to next page
                query.offset += query.limit;
                continue;
            }
            Some(SearchAction::PrevPage) => {
                // Go to previous page
                query.offset = query.offset.saturating_sub(query.limit);
                continue;
            }
            None => {
                cliclack::outro("Search cancelled")?;
                println!(); // Empty line at the end
                return Ok(());
            }
        }
    }
}

/// Run interactive search mode with sequential filter prompts
async fn run_interactive_search(data_dir: &std::path::PathBuf) -> Result<()> {
    use cliclack::{input, intro, outro, confirm};
    
    intro("Interactive Radio Search")?;
    
    let mut query = radm::search::SearchQuery::default();
    
    // Get search filters from user in specified order: name, language, country, tags, offset, and the rest
    
    // 1. Name
    let name_input: String = input("Station name")
        .default_input("")
        .interact()?;
    if !name_input.is_empty() {
        query.name = Some(name_input);
    }
    
    // 2. Language
    let language_input: String = input("Language")
        .default_input("")
        .interact()?;
    if !language_input.is_empty() {
        query.language = Some(language_input);
    }
    
    // 3. Country
    let country_input: String = input("Country")
        .default_input("")
        .interact()?;
    if !country_input.is_empty() {
        query.country = Some(country_input);
    }
    
    // 4. Tags
    let tags_input: String = input("Tags")
        .default_input("")
        .interact()?;
    if !tags_input.is_empty() {
        query.tags = Some(vec![tags_input]);
    }
    
    // 5. Offset
    let offset_input: String = input("Offset")
        .default_input("0")
        .interact()?;
    if let Ok(val) = offset_input.parse::<usize>() {
        query.offset = val;
    }
    
    // 6. Rest of the filters
    
    // Codec
    let codec_input: String = input("Codec")
        .default_input("")
        .interact()?;
    if !codec_input.is_empty() {
        query.codec = Some(codec_input);
    }
    
    // State
    let state_input: String = input("State")
        .default_input("")
        .interact()?;
    if !state_input.is_empty() {
        query.state = Some(state_input);
    }
    
    // Country code
    let countrycode_input: String = input("Country code")
        .default_input("")
        .interact()?;
    if !countrycode_input.is_empty() {
        query.countrycode = Some(countrycode_input);
    }
    
    // Bitrate min
    let bitrate_min_input: String = input("Minimum bitrate")
        .default_input("")
        .interact()?;
    if !bitrate_min_input.is_empty() {
        if let Ok(val) = bitrate_min_input.parse::<u32>() {
            query.bitrate_min = Some(val);
        }
    }
    
    // Bitrate max
    let bitrate_max_input: String = input("Maximum bitrate")
        .default_input("")
        .interact()?;
    if !bitrate_max_input.is_empty() {
        if let Ok(val) = bitrate_max_input.parse::<u32>() {
            query.bitrate_max = Some(val);
        }
    }
    
    // Order
    let order_input: String = input("Order by (e.g., name, votes, clickcount)")
        .default_input("")
        .interact()?;
    if !order_input.is_empty() {
        query.order = Some(order_input);
    }
    
    // Reverse
    let reverse_confirm = confirm("Reverse order?")
        .initial_value(false)
        .interact()?;
    if reverse_confirm {
        query.reverse = Some(true);
    }
    
    // Hide broken
    let hidebroken_confirm = confirm("Hide broken stations?")
        .initial_value(true)
        .interact()?;
    query.hidebroken = Some(hidebroken_confirm);
    
    // HTTPS only
    let https_confirm = confirm("HTTPS only?")
        .initial_value(false)
        .interact()?;
    if https_confirm {
        query.is_https = Some(true);
    }
    
    // Pagination loop
    loop {
        // Perform search with proper error handling for spinner
        let spinner = cliclack::spinner();
        spinner.start("Fetching results...");
        
        let results = match async {
            let mut api_client = radm::api::RadioBrowserClient::new().await?;
            api_client.advanced_search(&query).await
        }.await {
            Ok(results) => {
                spinner.stop("Results loaded");
                // Give terminal a moment to settle after spinner stops
                std::thread::sleep(std::time::Duration::from_millis(100));
                results
            }
            Err(e) => {
                spinner.error("Failed to fetch results");
                outro("Search failed")?;
                return Err(e);
            }
        };
        
        if results.is_empty() && query.offset == 0 {
            println!("No stations found");
            outro("Search completed")?;
            return Ok(());
        }
        
        // Interactive selection
        match run_interactive_search_with_select(results, &query).await? {
            Some(SearchAction::Play(station_name, station_url)) => {
                // Show spinner while loading station
                let spinner = cliclack::spinner();
                spinner.start(format!("Loading station: {}", station_name));
                
                // Play the selected station
                let daemon_client = radm::PlayerDaemonClient::new()?;
                let mut new_conn = daemon_client.connect().await?;
                
                if let Err(e) = new_conn.play(station_name.clone(), station_url.clone()).await {
                    spinner.error("Failed to play station");
                    eprintln!("Error: Failed to play station: {}", e);
                    return Err(e);
                }
                
                // Wait until the station is actually playing
                let mut attempts = 0;
                let max_attempts = 50; // 10 seconds max (50 * 200ms)
                loop {
                    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
                    
                    match new_conn.get_status().await {
                        Ok(status) => {
                            use radm::player::PlayerState;
                            match status.state {
                                PlayerState::Playing => {
                                    // Stream is loaded and playing!
                                    break;
                                }
                                PlayerState::Error => {
                                    spinner.error("Failed to load station");
                                    if let Some(err_msg) = status.error_message {
                                        eprintln!("Error: {}", err_msg);
                                    }
                                    return Err(anyhow::anyhow!("Station failed to load"));
                                }
                                PlayerState::Loading => {
                                    // Still loading, continue waiting
                                }
                                _ => {
                                    // Unexpected state, but continue waiting
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!("Failed to get status while waiting: {}", e);
                        }
                    }
                    
                    attempts += 1;
                    if attempts >= max_attempts {
                        spinner.error("Timed out waiting for station to load");
                        return Err(anyhow::anyhow!("Station took too long to load"));
                    }
                }
                
                // Explicitly drop the connection
                drop(new_conn);
                
                // Save as last played
                let mut config = Config::load(data_dir)?;
                config.update_session_state(config.default_volume, Some(station_name.clone()), Some(station_url));
                if let Err(e) = config.save(data_dir) {
                    tracing::warn!("Failed to save config: {}", e);
                }
                
                // Show final message with spinner stop
                spinner.stop(format!("Playing {}", station_name));
                println!(); // Empty line at the end
                return Ok(());
            }
            Some(SearchAction::NextPage) => {
                // Go to next page
                query.offset += query.limit;
                continue;
            }
            Some(SearchAction::PrevPage) => {
                // Go to previous page
                query.offset = query.offset.saturating_sub(query.limit);
                continue;
            }
            None => {
                outro("Search cancelled")?;
                println!(); // Empty line at the end
                return Ok(());
            }
        }
    }
}

fn print_help() {
    println!("\n╭────────────────────────────────────────────────────────────┐");
    println!("│ Radm - Radio Player Control                                │");
    println!("├────────────────────────────────────────────────────────────┤");
    println!("│ Usage: radc <command> [options]                            │");
    println!("│                                                            │");
    println!("│ Commands:                                                  │");
     println!("│   info                - Show current player status         │");
     println!("│   pause               - Pause playback                     │");
     println!("│   start               - Start playback                     │");
     println!("│   zap                 - Stop playback (kill daemon)         │");
    println!("│   volume <0-100>      - Set volume (0-100%)                │");
    println!("│   volume --up [amt]   - Increase volume (default 10%)      │");
    println!("│   volume --down [amt] - Decrease volume (default 10%)      │");
    println!("│ Search:                                                    │");
    println!("│   find                - Interactive mode with prompts      │");
    println!("│   find <query>        - Direct search (e.g., jazz)         │");
    println!("│   find --country X    - Filter by country/language/codec   │");
    println!("│   find --limit 20     - Set result limit (default: 100)    │");
    println!("│   find --skip N       - Paginate results (N results)       │");
    println!("│ Other:                                                     │");
    println!("│   quit/exit           - Exit CLI                           │");
    println!("│   help                - Show this help message             │");
    println!("╰────────────────────────────────────────────────────────────╯\n");
}
