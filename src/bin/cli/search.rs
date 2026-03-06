//! Interactive and non-interactive search functionality for CLI

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{enable_raw_mode, disable_raw_mode};
use std::{io::{self, Write}, time::Duration};
use lazyradio::{
    api::{RadioBrowserClient, Station},
    config::get_data_dir,
    search::SearchQuery,
    storage::FavoritesManager,
};

/// Cache for search results during a single search session
pub struct SearchCache {
    results: Vec<Station>,
    current_offset: usize,
    last_limit: usize,
}

impl SearchCache {
    fn new() -> Self {
        Self {
            results: Vec::new(),
            current_offset: 0,
            last_limit: 0,
        }
    }

    fn update(&mut self, new_results: Vec<Station>, limit: usize, offset: usize) {
        self.results = new_results;
        self.current_offset = offset;
        self.last_limit = limit;
    }

    fn has_more(&self) -> bool {
        // If we got a full page of results, assume there might be more
        self.results.len() >= self.last_limit && self.last_limit > 0
    }
}

/// State for interactive search session
pub struct InteractiveSearch {
    selected_index: usize,
    cache: SearchCache,
    favorites: FavoritesManager,
}

impl InteractiveSearch {
    pub async fn new() -> Result<Self> {
        let data_dir = get_data_dir()?;
        let favorites = FavoritesManager::new(&data_dir)?;

        Ok(Self {
            selected_index: 0,
            cache: SearchCache::new(),
            favorites,
        })
    }

    /// Display formatted station list with current selection
    fn display_stations(&self, terminal_width: u16) {
        clear_screen();
        println!("{}", format_station_list(
            &self.cache.results,
            self.selected_index,
            &self.favorites,
            terminal_width as usize,
            self.cache.current_offset,
        ));

        // Show pagination info
        let total_shown = self.cache.current_offset + self.cache.results.len();
        println!("\n(Showing results {}-{})", self.cache.current_offset + 1, total_shown);

        if self.cache.has_more() {
            println!("Press [n] for next page");
        }

        println!("\nCommands: [↑↓/jk] Navigate | [F] Favorite | [V] Vote | [Enter] Play | [n] Next | [q] Quit");
        print!("\n> ");
        let _ = io::stdout().flush();
    }

    /// Load search results
    pub async fn load_results(&mut self, query: SearchQuery) -> Result<()> {
        print!("Fetching");
        io::stdout().flush()?;

        let mut api_client = RadioBrowserClient::new().await?;
        let results = api_client.advanced_search(&query).await?;

        println!(" ✓");

        if results.is_empty() {
            println!("\nNo stations found");
            return Ok(());
        }

        self.cache.update(results, query.limit, query.offset);
        self.selected_index = 0;
        Ok(())
    }

    /// Load next page of results
    pub async fn load_next_page(&mut self, mut query: SearchQuery) -> Result<()> {
        query.offset = self.cache.current_offset + self.cache.results.len();
        self.load_results(query).await
    }

    /// Handle interactive navigation and actions
    pub async fn run(&mut self, query: SearchQuery, terminal_width: u16) -> Result<Option<(String, String)>> {
        // Enable raw mode for keyboard input
        enable_raw_mode()?;
        
        let result = self.run_inner(query, terminal_width).await;
        
        // Disable raw mode on exit (important for cleanup)
        let _ = disable_raw_mode();
        
        result
    }

    /// Inner loop for interactive navigation (separated for proper cleanup)
    async fn run_inner(&mut self, query: SearchQuery, terminal_width: u16) -> Result<Option<(String, String)>> {
        self.load_results(query.clone()).await?;

        loop {
            self.display_stations(terminal_width);

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key_event) = event::read()? {
                    match key_event.code {
                        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                            break;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if self.selected_index > 0 {
                                self.selected_index -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if self.selected_index < self.cache.results.len() - 1 {
                                self.selected_index += 1;
                            }
                        }
                        KeyCode::Char('f') | KeyCode::Char('F') => {
                            if let Some(station) = self.cache.results.get(self.selected_index) {
                                if self.favorites.is_favorite(&station.station_uuid) {
                                    let _ = self.favorites.remove(&station.station_uuid);
                                    println!("\n✓ Removed from favorites");
                                } else {
                                    let _ = self.favorites.add(station);
                                    println!("\n✓ Added to favorites");
                                }
                                tokio::time::sleep(Duration::from_millis(1000)).await;
                            }
                        }
                        KeyCode::Char('v') | KeyCode::Char('V') => {
                            if let Some(station) = self.cache.results.get(self.selected_index) {
                                match RadioBrowserClient::new().await {
                                    Ok(mut api) => {
                                        match api.vote_for_station(&station.station_uuid).await {
                                            Ok(_) => {
                                                println!("\n✓ Voted for: {}", station.name);
                                            }
                                            Err(e) => {
                                                println!("\n✗ Vote failed: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        println!("\n✗ API error: {}", e);
                                    }
                                }
                                tokio::time::sleep(Duration::from_millis(1000)).await;
                            }
                        }
                        KeyCode::Char('n') | KeyCode::Char('N') => {
                            if self.cache.has_more() {
                                self.load_next_page(query.clone()).await?;
                            }
                        }
                        KeyCode::Enter => {
                            if let Some(station) = self.cache.results.get(self.selected_index) {
                                return Ok(Some((station.name.clone(), station.url_resolved.clone())));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(None)
    }
}

/// Format station list for display with terminal width adaptation
pub fn format_station_list(
    stations: &[Station],
    selected: usize,
    favorites: &FavoritesManager,
    terminal_width: usize,
    start_index: usize,
) -> String {
    if stations.is_empty() {
        return "No stations to display".to_string();
    }

    let mut output = String::new();

    // Calculate column widths based on terminal width
    let min_width = 80;
    let width = terminal_width.max(min_width);

    // Fixed columns: index(3) + fav(2) + country(4) + lang(4) + codec(5) + votes(7) = 25
    // Variable: name gets remaining space
    let fixed_width = 25;
    let name_width = width.saturating_sub(fixed_width).max(20);

    output.push_str(&format!("\n{:<3} {:<2} {:<20} {:<4} {:<4} {:<5} {:<7}\n",
        "No.", "★", "Station", "Cntr", "Lang", "Codec", "Votes"));
    output.push_str(&"─".repeat(width.min(120)));
    output.push('\n');

    for (idx, station) in stations.iter().enumerate() {
        let global_idx = start_index + idx + 1;
        let is_selected = idx == selected;
        let fav_icon = if favorites.is_favorite(&station.station_uuid) { "★" } else { "☆" };
        
        let prefix = if is_selected { "> " } else { "  " };
        let name = truncate_string(&station.name, name_width);
        let country = truncate_string(&station.country, 4);
        let language = truncate_string(&station.language, 4);
        let codec = truncate_string(&station.codec, 5);
        let votes = format!("{}", station.votes);

        let line = format!("{}{:<2} {} {:<20} {:<4} {:<4} {:<5} {:<7}",
            prefix, fav_icon, global_idx, name, country, language, codec, votes);

        output.push_str(&line);
        output.push('\n');
    }

    output
}

/// Truncate string with ellipsis if needed
fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len.saturating_sub(1)])
    } else {
        s.to_string()
    }
}

/// Clear terminal screen
fn clear_screen() {
    print!("\x1B[2J\x1B[1;1H");
    let _ = io::stdout().flush();
}

/// Parse CLI arguments into SearchQuery
pub fn parse_search_args(args: &[String]) -> SearchQuery {
    let mut query = SearchQuery::default();
    let mut i = 2; // Skip program name and "search" command

    while i < args.len() {
        let arg = &args[i];

        match arg.as_str() {
            "--name" if i + 1 < args.len() => {
                query.name = Some(args[i + 1].clone());
                i += 2;
            }
            "--country" if i + 1 < args.len() => {
                query.country = Some(args[i + 1].clone());
                i += 2;
            }
            "--countrycode" if i + 1 < args.len() => {
                query.countrycode = Some(args[i + 1].clone());
                i += 2;
            }
            "--language" if i + 1 < args.len() => {
                query.language = Some(args[i + 1].clone());
                i += 2;
            }
            "--tags" if i + 1 < args.len() => {
                query.tags = Some(vec![args[i + 1].clone()]);
                i += 2;
            }
            "--codec" if i + 1 < args.len() => {
                query.codec = Some(args[i + 1].clone());
                i += 2;
            }
            "--state" if i + 1 < args.len() => {
                query.state = Some(args[i + 1].clone());
                i += 2;
            }
            "--bitrate-min" if i + 1 < args.len() => {
                if let Ok(val) = args[i + 1].parse() {
                    query.bitrate_min = Some(val);
                }
                i += 2;
            }
            "--bitrate-max" if i + 1 < args.len() => {
                if let Ok(val) = args[i + 1].parse() {
                    query.bitrate_max = Some(val);
                }
                i += 2;
            }
            "--order" if i + 1 < args.len() => {
                query.order = Some(args[i + 1].clone());
                i += 2;
            }
            "--reverse" => {
                query.reverse = Some(true);
                i += 1;
            }
            "--no-reverse" => {
                query.reverse = Some(false);
                i += 1;
            }
            "--hidebroken" => {
                query.hidebroken = Some(true);
                i += 1;
            }
            "--show-broken" => {
                query.hidebroken = Some(false);
                i += 1;
            }
            "--https-only" => {
                query.is_https = Some(true);
                i += 1;
            }
            "--limit" if i + 1 < args.len() => {
                if let Ok(val) = args[i + 1].parse() {
                    query.limit = val;
                }
                i += 2;
            }
            "--skip" if i + 1 < args.len() => {
                if let Ok(val) = args[i + 1].parse() {
                    query.offset = val;
                }
                i += 2;
            }
            arg if !arg.starts_with("--") => {
                // Positional argument = name filter
                if query.name.is_none() {
                    query.name = Some(arg.to_string());
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    query
}
