mod api;
mod app;
mod config;
mod player;
mod search;
mod storage;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::time::interval;
use tracing::info;
use tracing_subscriber;

use app::{App, BrowseMode};
use config::get_data_dir;
use player::{AudioPlayer, PlayerCommand};
use search::{get_suggestions, parse_query};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "web-radio.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    info!("Starting Web Radio TUI");

    // Try to initialize API client
    let api_client_result = api::RadioBrowserClient::new().await;
    let api_client = match api_client_result {
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

    // Initialize audio player (with error handling)
    let audio_player_result = AudioPlayer::new();
    let (mut audio_player, mut player_cmd_rx) = match audio_player_result {
        Ok((player, rx)) => {
            info!("Audio player initialized");
            (player, rx)
        }
        Err(e) => {
            // Initialize app without audio player to show error
            let mut app = App::new(data_dir.clone(), api_client).await?;
            app.show_error(format!("Failed to initialize audio device: {}\n\nPlease check that your audio output is properly configured.", e));
            
            // Load initial data (popular stations with default query)
            if let Err(load_err) = app.execute_search().await {
                tracing::warn!("Failed to load initial stations: {}", load_err);
            }
            
            // Setup terminal to show error
            enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend)?;
            
            // Show error screen until user closes it
            loop {
                terminal.draw(|f| ui::draw(f, &mut app))?;
                
                if event::poll(Duration::from_millis(100))? {
                    if let Event::Key(key) = event::read()? {
                        if matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') | KeyCode::Char('Q')) {
                            break;
                        }
                    }
                }
            }
            
            // Cleanup
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;
            
            return Ok(());
        }
    };

    // Initialize app (without player)
    let mut app = App::new(data_dir, api_client).await?;
    let player_cmd_tx = audio_player.get_command_sender();
    app.player_cmd_tx = player_cmd_tx.clone();
    
    // Set restored volume in audio player
    let restored_volume = app.player_info.volume;
    if let Err(e) = player_cmd_tx.send(PlayerCommand::SetVolume(restored_volume)) {
        tracing::warn!("Failed to set restored volume: {}", e);
    }
    
    info!("App initialized");

    // Load initial data (popular stations with default query)
    if let Err(e) = app.execute_search().await {
        tracing::error!("Failed to load initial stations: {}", e);
        app.status_message = Some(format!("Failed to load stations: {}. Check network connection.", e));
    }
    
    tracing::info!("Initial data loaded. Stations count: {}", app.stations.len());
    
    // Restore last session if available
    if let (Some(station_name), Some(_station_url)) = (&app.config.last_station_name, &app.config.last_station_url) {
        app.add_log(format!("Restored last session: {}", station_name));
        // Station info is already set in PlayerInfo from App::new()
        // State is Stopped, so it will show but not auto-play
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let mut tick_interval = interval(Duration::from_millis(100));

    loop {
        // Handle player commands
        while let Ok(cmd) = player_cmd_rx.try_recv() {
            tracing::debug!("Received player command: {:?}", match &cmd {
                PlayerCommand::Play(name, _) => format!("Play({})", name),
                PlayerCommand::Pause => "Pause".to_string(),
                PlayerCommand::Resume => "Resume".to_string(),
                PlayerCommand::Stop => "Stop".to_string(),
                PlayerCommand::SetVolume(v) => format!("SetVolume({})", v),
                PlayerCommand::Reload => "Reload".to_string(),
            });
            
            match cmd {
                PlayerCommand::Play(name, url) => {
                    tracing::info!("Processing Play command for: {}", name);
                    audio_player.play(name, url);
                }
                PlayerCommand::Pause => audio_player.pause(),
                PlayerCommand::Resume => audio_player.resume(),
                PlayerCommand::Stop => audio_player.stop(),
                PlayerCommand::SetVolume(vol) => audio_player.set_volume(vol),
                PlayerCommand::Reload => {
                    audio_player.reload();
                }
            }
        }

        // Update player info in app
        let audio_info = audio_player.get_info();
        
        // Preserve restored station info if player hasn't started playing yet
        // (audio player starts with empty station_name/url)
        if audio_info.station_name.is_empty() && !app.player_info.station_name.is_empty() {
            // Keep the restored station info, only update state and volume
            app.player_info.state = audio_info.state;
            app.player_info.volume = audio_info.volume;
            app.player_info.error_message = audio_info.error_message;
        } else {
            // Normal case: audio player has station info, use it
            app.player_info = audio_info;
        }
        
        // Show error popup if player has error
        if let Some(ref err_msg) = app.player_info.error_message {
            if app.error_popup.is_none() {
                app.show_error(err_msg.clone());
            }
        }

        // Update animation frame
        app.animation_frame = (app.animation_frame + 1) % 8;

        // Draw UI
        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Execute pending search if flag is set
        if app.pending_search {
            app.pending_search = false;
            tracing::info!("Executing pending search");
            if let Err(e) = app.execute_search().await {
                tracing::error!("Search failed: {}", e);
                app.show_error(format!("Search failed: {}", e));
            }
        }

        // Handle events
        tokio::select! {
            _ = tick_interval.tick() => {
                // Regular tick for animations
            }
            _ = async {
                if event::poll(Duration::from_millis(50)).unwrap() {
                    if let Ok(Event::Key(key)) = event::read() {
                        handle_key_event(&mut app, key.code, key.modifiers).await;
                    }
                }
            } => {}
        }

        if !app.running {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    info!("Web Radio TUI shutting down");

    Ok(())
}

async fn handle_key_event(app: &mut App, key: KeyCode, modifiers: KeyModifiers) {
    // Handle Ctrl+C to quit immediately
    if modifiers.contains(KeyModifiers::CONTROL) && matches!(key, KeyCode::Char('c')) {
        app.quit();
        return;
    }
    
    // Handle help popup first
    if app.help_popup {
        match key {
            KeyCode::Esc | KeyCode::Char('?') => {
                app.help_popup = false;
            }
            _ => {}
        }
        return;
    }
    
    // Handle error popup (takes priority after help)
    if app.error_popup.is_some() {
        match key {
            KeyCode::Esc | KeyCode::Enter => {
                app.close_error_popup();
            }
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                // Allow quit even with error popup
                app.quit();
            }
            _ => {}
        }
        return;
    }
    
    // Handle search popup
    if app.search_popup.is_some() {
        match key {
            KeyCode::Char(c) => {
                if let Some(popup) = &mut app.search_popup {
                    popup.insert_char(c);
                    let suggestions = get_suggestions(&popup.input, popup.cursor_position, &app.autocomplete_data);
                    popup.update_autocomplete(suggestions);
                }
            }
            KeyCode::Backspace => {
                if let Some(popup) = &mut app.search_popup {
                    popup.delete_char();
                    let suggestions = get_suggestions(&popup.input, popup.cursor_position, &app.autocomplete_data);
                    popup.update_autocomplete(suggestions);
                }
            }
            KeyCode::Tab => {
                if let Some(popup) = &mut app.search_popup {
                    popup.accept_autocomplete();
                    let suggestions = get_suggestions(&popup.input, popup.cursor_position, &app.autocomplete_data);
                    popup.update_autocomplete(suggestions);
                }
            }
            KeyCode::Down => {
                if let Some(popup) = &mut app.search_popup {
                    if popup.autocomplete_shown {
                        popup.autocomplete_next();
                    }
                }
            }
            KeyCode::Up => {
                if let Some(popup) = &mut app.search_popup {
                    if popup.autocomplete_shown {
                        popup.autocomplete_prev();
                    }
                }
            }
            KeyCode::Enter => {
                if let Some(popup) = &mut app.search_popup {
                    // Always execute search when Enter is pressed
                    let query_str = popup.get_query();
                    tracing::info!("Enter pressed in popup, query: '{}'", query_str);
                    match parse_query(query_str) {
                        Ok(query) => {
                            tracing::info!("Query parsed successfully: {:?}", query);
                            app.current_query = query;
                            app.close_search_popup();
                            tracing::info!("Popup closed, triggering pending_search");
                            // Set flag to execute search on next loop iteration
                            app.pending_search = true;
                        }
                        Err(e) => {
                            tracing::error!("Query parse error: {:?}", e);
                            app.show_error(format!("Invalid query: {}", e));
                            // Keep popup open on error
                        }
                    }
                }
            }
            KeyCode::Esc => {
                if let Some(popup) = &mut app.search_popup {
                    if popup.autocomplete_shown {
                        // First Esc closes autocomplete
                        popup.autocomplete_shown = false;
                    } else {
                        // Second Esc closes popup
                        app.close_search_popup();
                    }
                }
            }
            _ => {}
        }
        return;
    }

    if app.browse_list_mode {
        match key {
            KeyCode::Up => app.select_prev(),
            KeyCode::Down => app.select_next(),
            KeyCode::PageUp => app.page_up(),
            KeyCode::PageDown => app.page_down(),
            KeyCode::Enter => {
                if let Err(e) = app.select_from_browse_list().await {
                    tracing::error!("Failed to load stations: {}", e);
                    app.show_error(format!("Failed to load stations: {}", e));
                }
            }
            KeyCode::Esc => app.browse_list_mode = false,
            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
            _ => {}
        }
        return;
    }

    match key {
        KeyCode::Char('?') => {
            app.help_popup = true;
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        KeyCode::Up => app.select_prev(),
        KeyCode::Down => app.select_next(),
        KeyCode::PageUp => {
            // If we're viewing search results with pagination, go to previous page
            if app.current_page > 1 {
                if let Err(e) = app.prev_page().await {
                    tracing::error!("Failed to load previous page: {}", e);
                    app.show_error(format!("Failed to load page: {}", e));
                }
            } else {
                // Otherwise, page up within the current list
                app.page_up();
            }
        }
        KeyCode::PageDown => {
            // If we're viewing search results with pagination, go to next page
            if !app.is_last_page {
                if let Err(e) = app.next_page().await {
                    tracing::error!("Failed to load next page: {}", e);
                    app.show_error(format!("Failed to load page: {}", e));
                }
            } else {
                // Otherwise, page down within the current list
                app.page_down();
            }
        }
        KeyCode::Home => {
            // Go to first page of search results
            if app.current_page > 1 {
                if let Err(e) = app.first_page().await {
                    tracing::error!("Failed to load first page: {}", e);
                    app.show_error(format!("Failed to load page: {}", e));
                }
            }
        }
        KeyCode::Enter => {
            if let Err(e) = app.play_selected().await {
                tracing::error!("Failed to play station: {}", e);
                app.show_error(format!("Failed to play station: {}", e));
            }
        }
        KeyCode::Char(' ') => {
            if app.player_info.state == player::PlayerState::Playing {
                let _ = app.pause();
            } else {
                let _ = app.resume();
            }
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            let _ = app.stop();
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let _ = app.reload();
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let _ = app.volume_up();
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            let _ = app.volume_down();
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            if let Err(e) = app.toggle_favorite().await {
                tracing::error!("Failed to toggle favorite: {}", e);
            }
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            if let Err(e) = app.vote_for_selected().await {
                tracing::error!("Failed to vote: {}", e);
            }
        }
        KeyCode::Char('/') => app.open_search_popup(),
        KeyCode::F(1) => {
            // F1 to reload default query (popular stations)
            if let Err(e) = app.first_page().await {
                tracing::error!("Failed to reload stations: {}", e);
                app.show_error(format!("Failed to reload stations: {}", e));
            }
        }
        KeyCode::Tab => {
            if modifiers.contains(KeyModifiers::SHIFT) {
                app.prev_tab();
            } else {
                app.next_tab();
            }
        }
        KeyCode::Char('[') => {
            app.prev_tab();
        }
        KeyCode::Char(']') => {
            app.next_tab();
        }
        KeyCode::Char('1') => {
            if app.current_tab == app::Tab::Browse {
                app.set_browse_mode(BrowseMode::Popular);
                if let Err(e) = app.load_popular_stations().await {
                    tracing::error!("Failed to load popular stations: {}", e);
                }
            } else {
                app.current_tab = app::Tab::Browse;
                app.reload_current_tab();
            }
        }
        KeyCode::Char('2') => {
            if app.current_tab == app::Tab::Browse {
                app.set_browse_mode(BrowseMode::ByCountry);
                app.browse_list_mode = true;
            } else {
                app.current_tab = app::Tab::Favorites;
                app.reload_current_tab();
            }
        }
        KeyCode::Char('3') => {
            if app.current_tab == app::Tab::Browse {
                app.set_browse_mode(BrowseMode::ByGenre);
                app.browse_list_mode = true;
            } else {
                app.current_tab = app::Tab::History;
                app.reload_current_tab();
            }
        }
        KeyCode::Char('4') => {
            if app.current_tab != app::Tab::Browse {
                app.current_tab = app::Tab::Browse;
                app.reload_current_tab();
            }
        }
        KeyCode::F(2) => {
            app.set_browse_mode(BrowseMode::ByCountry);
            app.browse_list_mode = true;
        }
        KeyCode::F(3) => {
            app.set_browse_mode(BrowseMode::ByGenre);
            app.browse_list_mode = true;
        }
        KeyCode::F(4) => {
            app.set_browse_mode(BrowseMode::ByLanguage);
            app.browse_list_mode = true;
        }
        _ => {}
    }
}
