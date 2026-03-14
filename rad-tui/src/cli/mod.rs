mod search;

use anyhow::Result;
use std::path::PathBuf;
use tracing::info;

use rad_core::{
    config::Config,
    api::RadioBrowserClient,
    PlayerDaemonClient,
};

use search::{parse_search_args, run_interactive_search_with_select, SearchAction};

pub async fn run(args: Vec<String>, data_dir: &PathBuf) -> Result<()> {
    info!("CLI mode, args: {:?}", &args[1..]);

    match args.get(1).map(|s| s.as_str()) {
        Some("find") => {
            if args.len() <= 2 {
                run_interactive_search(data_dir).await?;
            } else {
                run_direct_search(&args, data_dir).await?;
            }
        }
        Some("help") | Some("-h") | Some("--help") => {
            print_help();
        }
        Some("completion") => {
            print_completion_zsh();
        }
        _ => {
            // All other commands require a daemon connection
            let daemon_client = PlayerDaemonClient::new()?;
            let mut daemon_conn = match daemon_client.connect().await {
                Ok(conn) => conn,
                Err(e) => {
                    eprintln!("Error: Failed to connect to player daemon: {}", e);
                    return Err(e);
                }
            };

            match args.get(1).map(|s| s.as_str()) {
                None | Some("info") => {
                    match daemon_conn.get_status().await {
                        Ok(info) => {
                            println!("\nPlayer Status:");
                            println!("  State:   {}", match info.state {
                                rad_core::PlayerState::Playing => "Playing",
                                rad_core::PlayerState::Paused  => "Paused",
                                rad_core::PlayerState::Stopped => "Stopped",
                                rad_core::PlayerState::Loading => "Loading",
                                rad_core::PlayerState::Error   => "Error",
                            });
                            println!("  Station: {}", if info.station_name.is_empty() { "None" } else { &info.station_name });
                            println!("  Volume:  {:.0}%", info.volume * 100.0);
                            if let Some(err) = &info.error_message {
                                println!("  Error:   {}", err);
                            }
                            println!();
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            return Err(e);
                        }
                    }
                }
                Some("stop") => {
                    let info = daemon_conn.get_status().await
                        .map_err(|e| { eprintln!("Error: {}", e); e })?;
                    if info.state == rad_core::PlayerState::Paused {
                        println!("Already paused");
                    } else {
                        daemon_conn.pause().await.map_err(|e| { eprintln!("Error: {}", e); e })?;
                        let station = if info.station_name.is_empty() { "playback".to_string() } else { info.station_name.clone() };
                        println!("Paused: {}", station);
                    }
                }
                Some("play") => {
                    match daemon_conn.get_status().await {
                        Ok(info) => {
                            if info.state == rad_core::PlayerState::Paused {
                                daemon_conn.resume().await.map_err(|e| { eprintln!("Error: {}", e); e })?;
                                let station = if info.station_name.is_empty() { "playback".to_string() } else { info.station_name.clone() };
                                println!("Resumed: {}", station);
                            } else if info.state == rad_core::PlayerState::Stopped {
                                let config = Config::load(data_dir)?;
                                if let (Some(name), Some(url)) = (config.last_station_name, config.last_station_url) {
                                    daemon_conn.play(name.clone(), url).await
                                        .map_err(|e| { eprintln!("Error: {}", e); e })?;
                                    println!("Playing: {}", name);
                                } else {
                                    println!("No station to play — use 'rad find' to search for stations");
                                }
                            } else {
                                let station = if info.station_name.is_empty() { "playback".to_string() } else { info.station_name.clone() };
                                println!("Already playing: {}", station);
                            }
                        }
                        Err(e) => {
                            eprintln!("Error: {}", e);
                            return Err(e);
                        }
                    }
                }
                Some("kill") => {
                    let info = daemon_conn.get_status().await
                        .map_err(|e| { eprintln!("Error: {}", e); e })?;
                    daemon_conn.shutdown().await.map_err(|e| { eprintln!("Error: {}", e); e })?;
                    let station = if info.station_name.is_empty() { String::new() } else { format!(": {}", info.station_name) };
                    println!("Stopped{}", station);
                }
                Some("volume") => {
                    match args.get(2).map(|s| s.as_str()) {
                        Some("--up") => {
                            let amount = args.get(3)
                                .and_then(|s| s.parse::<f32>().ok())
                                .map(|v| v / 100.0)
                                .unwrap_or(0.1);
                            let info = daemon_conn.get_status().await
                                .map_err(|e| { eprintln!("Error: {}", e); e })?;
                            let new_vol = (info.volume + amount).min(1.0);
                            daemon_conn.set_volume(new_vol).await
                                .map_err(|e| { eprintln!("Error: {}", e); e })?;
                            println!("Volume: {:.0}% → {:.0}%", info.volume * 100.0, new_vol * 100.0);
                        }
                        Some("--down") => {
                            let amount = args.get(3)
                                .and_then(|s| s.parse::<f32>().ok())
                                .map(|v| v / 100.0)
                                .unwrap_or(0.1);
                            let info = daemon_conn.get_status().await
                                .map_err(|e| { eprintln!("Error: {}", e); e })?;
                            let new_vol = (info.volume - amount).max(0.0);
                            daemon_conn.set_volume(new_vol).await
                                .map_err(|e| { eprintln!("Error: {}", e); e })?;
                            println!("Volume: {:.0}% → {:.0}%", info.volume * 100.0, new_vol * 100.0);
                        }
                        Some(vol_str) => {
                            if let Ok(vol_percent) = vol_str.parse::<f32>() {
                                let info = daemon_conn.get_status().await
                                    .map_err(|e| { eprintln!("Error: {}", e); e })?;
                                let vol = (vol_percent / 100.0).clamp(0.0, 1.0);
                                daemon_conn.set_volume(vol).await
                                    .map_err(|e| { eprintln!("Error: {}", e); e })?;
                                println!("Volume: {:.0}% → {:.0}%", info.volume * 100.0, vol * 100.0);
                            } else {
                                eprintln!("Error: Invalid volume value '{}'. Use 0-100, --up [amt], or --down [amt]", vol_str);
                                return Err(anyhow::anyhow!("Invalid volume argument"));
                            }
                        }
                        None => {
                            let info = daemon_conn.get_status().await
                                .map_err(|e| { eprintln!("Error: {}", e); e })?;
                            println!("Volume: {:.0}%", info.volume * 100.0);
                        }
                    }
                }
                Some(cmd) => {
                    eprintln!("Error: Unknown command '{}'.\n", cmd);
                    print_help();
                    std::process::exit(1);
                }
            }

            drop(daemon_conn);
        }
    }

    info!("CLI completed");
    Ok(())
}

async fn run_direct_search(args: &[String], data_dir: &PathBuf) -> Result<()> {
    let mut query = parse_search_args(args);

    loop {
        let spinner = cliclack::spinner();
        spinner.start("Fetching stations...");

        let results = match async {
            let mut api_client = RadioBrowserClient::new().await?;
            api_client.advanced_search(&query).await
        }.await {
            Ok(results) => {
                spinner.stop("Stations loaded");
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

        match run_interactive_search_with_select(results, &query).await? {
            Some(SearchAction::Play(name, url)) => {
                play_station(name, url, data_dir).await?;
                return Ok(());
            }
            Some(SearchAction::NextPage) => {
                query.offset += query.limit;
            }
            Some(SearchAction::PrevPage) => {
                query.offset = query.offset.saturating_sub(query.limit);
            }
            None => {
                cliclack::outro("Search cancelled")?;
                println!();
                return Ok(());
            }
        }
    }
}

async fn run_interactive_search(data_dir: &PathBuf) -> Result<()> {
    use cliclack::{confirm, input, intro, outro};

    intro("Interactive Radio Search")?;

    let mut query = rad_core::search::SearchQuery::default();

    let name_input: String = input("Station name").default_input("").interact()?;
    if !name_input.is_empty() { query.name = Some(name_input); }

    let language_input: String = input("Language").default_input("").interact()?;
    if !language_input.is_empty() { query.language = Some(language_input); }

    let country_input: String = input("Country").default_input("").interact()?;
    if !country_input.is_empty() { query.country = Some(country_input); }

    let tags_input: String = input("Tags").default_input("").interact()?;
    if !tags_input.is_empty() { query.tags = Some(vec![tags_input]); }

    let codec_input: String = input("Codec").default_input("").interact()?;
    if !codec_input.is_empty() { query.codec = Some(codec_input); }

    let order_input: String = input("Order by (e.g., name, votes, clickcount)").default_input("").interact()?;
    if !order_input.is_empty() { query.order = Some(order_input); }

    query.hidebroken = Some(confirm("Hide broken stations?").initial_value(true).interact()?);

    loop {
        let spinner = cliclack::spinner();
        spinner.start("Fetching results...");

        let results = match async {
            let mut api_client = rad_core::api::RadioBrowserClient::new().await?;
            api_client.advanced_search(&query).await
        }.await {
            Ok(results) => {
                spinner.stop("Results loaded");
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

        match run_interactive_search_with_select(results, &query).await? {
            Some(SearchAction::Play(name, url)) => {
                play_station(name, url, data_dir).await?;
                return Ok(());
            }
            Some(SearchAction::NextPage) => { query.offset += query.limit; }
            Some(SearchAction::PrevPage) => { query.offset = query.offset.saturating_sub(query.limit); }
            None => {
                outro("Search cancelled")?;
                println!();
                return Ok(());
            }
        }
    }
}

async fn play_station(name: String, url: String, data_dir: &PathBuf) -> Result<()> {
    let spinner = cliclack::spinner();
    spinner.start(format!("Loading: {}", name));

    let daemon_client = rad_core::PlayerDaemonClient::new()?;
    let mut conn = daemon_client.connect().await?;

    conn.play(name.clone(), url.clone()).await
        .map_err(|e| { spinner.error("Failed to play station"); e })?;

    // Wait for the stream to actually start
    let mut attempts = 0;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        match conn.get_status().await {
            Ok(status) => match status.state {
                rad_core::PlayerState::Playing => break,
                rad_core::PlayerState::Error => {
                    spinner.error("Failed to load station");
                    if let Some(msg) = status.error_message {
                        eprintln!("Error: {}", msg);
                    }
                    return Err(anyhow::anyhow!("Station failed to load"));
                }
                _ => {}
            },
            Err(e) => tracing::warn!("Status check failed: {}", e),
        }
        attempts += 1;
        if attempts >= 50 {
            spinner.error("Timed out");
            return Err(anyhow::anyhow!("Station took too long to load"));
        }
    }

    drop(conn);

    let mut config = Config::load(data_dir)?;
    config.update_session_state(config.default_volume, Some(name.clone()), Some(url));
    config.save(data_dir).ok();

    spinner.stop(format!("Playing {}", name));
    println!();
    Ok(())
}

fn print_help() {
    println!("\n╭────────────────────────────────────────────────────────────╮");
    println!("│ rad - Radio Player                                         │");
    println!("├────────────────────────────────────────────────────────────┤");
    println!("│ Usage: rad [command] [options]                             │");
    println!("│   (no args)           Launch the TUI                       │");
    println!("│                                                            │");
    println!("│ Playback:                                                  │");
    println!("│   info                Show current player status           │");
    println!("│   play                Resume or replay last station        │");
    println!("│   stop                Pause playback                       │");
    println!("│   kill                Kill the daemon and stop playback    │");
    println!("│   volume <0-100>      Set volume (0-100%)                  │");
    println!("│   volume --up [amt]   Increase volume (default 10%)        │");
    println!("│   volume --down [amt] Decrease volume (default 10%)        │");
    println!("│                                                            │");
    println!("│ Search:                                                    │");
    println!("│   find                Interactive search with prompts      │");
    println!("│   find <query>        Direct search (e.g., jazz)           │");
    println!("│   find --country X    Filter by country                    │");
    println!("│   find --language X   Filter by language                   │");
    println!("│   find --limit 20     Set result limit (default: 100)      │");
    println!("│   find --skip N       Paginate results                     │");
    println!("│                                                            │");
    println!("│ Other:                                                     │");
    println!("│   help                Show this help message               │");
    println!("│   completion          Print zsh completion script          │");
    println!("╰────────────────────────────────────────────────────────────╯\n");
}

fn print_completion_zsh() {
    print!("{}", r#"#compdef rad

_rad() {
  local -a commands
  commands=(
    'info:Show current player status'
    'play:Resume or replay last station'
    'stop:Pause playback'
    'kill:Kill the daemon and stop playback'
    'volume:Get or set volume'
    'find:Search for radio stations'
    'help:Show help'
    'completion:Print zsh completion script'
  )

  case $words[2] in
    volume)
      local -a vopts
      vopts=(
        '--up:Increase volume'
        '--down:Decrease volume'
      )
      _describe 'volume options' vopts
      ;;
    find)
      _arguments \
        ':query:' \
        '--country[Filter by country]:country' \
        '--language[Filter by language]:language' \
        '--tags[Filter by tags]:tags' \
        '--codec[Filter by codec]:codec' \
        '--limit[Set result limit]:limit' \
        '--skip[Skip N results]:offset' \
        '--order[Sort order (name/votes/clickcount)]:order:(name votes clickcount)' \
        '--hidebroken[Hide broken stations]'
      ;;
    *)
      _describe 'rad commands' commands
      ;;
  esac
}

_rad "$@"
"#);
}
