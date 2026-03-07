mod app;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::time::interval;
use tracing::info;
use tracing_subscriber;

use crate::app::App;
use radm::{
    config::{cleanup_old_logs, get_data_dir},
    search::{get_suggestions, parse_query},
    PlayerDaemonClient,
    RadioBrowserClient,
};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    let data_dir = get_data_dir()?;
    let log_file = tracing_appender::rolling::daily(&data_dir, "radt.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    info!("Starting Web Radio TUI");

    // Clean up old log files (older than 7 days)
    if let Err(e) = cleanup_old_logs(&data_dir, 7) {
        tracing::warn!("Failed to clean up old logs: {}", e);
    }

    // Initialize connection to player daemon FIRST (this should be instant)
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

    // Initialize API client with a short timeout (can take a few seconds for DNS lookup)
    let api_client = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        RadioBrowserClient::new()
    ).await {
        Ok(Ok(client)) => {
            info!("API client initialized");
            client
        }
        Ok(Err(e)) => {
            eprintln!("Failed to initialize Radio Browser API client: {}", e);
            eprintln!("Please check your internet connection and DNS configuration.");
            return Err(e);
        }
        Err(_) => {
            eprintln!("API client initialization timed out after 10 seconds");
            eprintln!("Please check your internet connection and DNS configuration.");
            return Err(anyhow::anyhow!("API client initialization timed out"));
        }
    };

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize app
    let mut app = App::new(data_dir, api_client, &mut daemon_conn).await?;

    // Get initial player status from daemon
    if let Ok(player_info) = daemon_conn.get_status().await {
        app.player_info = player_info;
        info!("Retrieved initial player status from daemon");
    }

    // Load initial data (popular stations with default query)
    if let Err(e) = app.execute_search().await {
        tracing::error!("Failed to load initial stations: {}", e);
        app.status_message = Some(format!("Failed to load stations: {}. Check network connection.", e));
    }

    tracing::info!("Initial data loaded. Stations count: {}", app.stations.len());

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Main loop
    let mut tick_interval = interval(Duration::from_millis(100));

    loop {
        // Update player info from daemon
        if let Ok(player_info) = daemon_conn.get_status().await {
            app.player_info = player_info;
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

        // Execute pending page change if flag is set
        if let Some(direction) = app.pending_page_change {
            app.pending_page_change = None;
            tracing::info!("Executing pending page change: direction={}", direction);
            if direction > 0 {
                // Next page
                if let Err(e) = app.next_page().await {
                    tracing::error!("Failed to load next page: {}", e);
                    app.show_error(format!("Failed to load page: {}", e));
                }
            } else {
                // Previous page
                if let Err(e) = app.prev_page().await {
                    tracing::error!("Failed to load previous page: {}", e);
                    app.show_error(format!("Failed to load page: {}", e));
                }
            }
        }

        // Handle events
        tokio::select! {
            _ = tick_interval.tick() => {
                // Regular tick for animations
            }
            _ = tokio::signal::ctrl_c() => {
                // Handle Ctrl+C signal from OS
                info!("Received Ctrl+C signal, shutting down...");
                app.quit();
            }
            _ = async {
                if event::poll(Duration::from_millis(50)).unwrap() {
                    if let Ok(Event::Key(key)) = event::read() {
                        // Only handle key press events, not release
                        if key.kind == KeyEventKind::Press {
                            handle_key_event(&mut app, &mut daemon_conn, key.code, key.modifiers).await;
                        }
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

async fn handle_key_event(
    app: &mut App,
    daemon_conn: &mut radm::PlayerDaemonConnection,
    key: KeyCode,
    modifiers: KeyModifiers,
) {
    tracing::info!("handle_key_event called with key: {:?}, modifiers: {:?}", key, modifiers);

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

    // Handle error/warning popup (takes priority after help)
    if app.error_popup.is_some() || app.warning_popup.is_some() {
        tracing::info!("Error/warning popup is open, key pressed: {:?}", key);
        match key {
            KeyCode::Esc | KeyCode::Enter => {
                tracing::info!("Closing error/warning popup");
                app.close_error_popup();
            }
            _ => {
                tracing::info!("Ignoring key: {:?}", key);
            }
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

    match key {
        KeyCode::Char('?') => {
            app.help_popup = true;
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        KeyCode::Up => app.select_prev(),
        KeyCode::Down => app.select_next(),
        KeyCode::PageUp => {
            // Scroll up within the current list by one page length
            app.page_up();
        }
        KeyCode::PageDown => {
            // Scroll down within the current list by one page length
            app.page_down();
        }
        KeyCode::Home => {
            // Jump to first station in current list
            app.selected_index = 0;
            app.scroll_offset = 0;
        }
        KeyCode::End => {
            // Jump to last station in current list
            if !app.stations.is_empty() {
                app.selected_index = app.stations.len() - 1;
            }
        }
        KeyCode::Enter => {
            if let Err(e) = app.play_selected(daemon_conn).await {
                tracing::error!("Failed to play station: {}", e);
                app.show_error(format!("Failed to play station: {}", e));
            }
        }
        KeyCode::Char(' ') => {
            if app.player_info.state == radm::PlayerState::Playing {
                let _ = app.pause(daemon_conn).await;
            } else if app.player_info.state == radm::PlayerState::Paused {
                let _ = app.resume(daemon_conn).await;
            } else if app.player_info.state == radm::PlayerState::Stopped && !app.player_info.station_url.is_empty() {
                // If stopped but there's a restored station, play it
                let _ = app.play_restored(daemon_conn).await;
            } else {
                // Otherwise try to resume (handles other edge cases)
                let _ = app.resume(daemon_conn).await;
            }
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            let _ = app.stop(daemon_conn).await;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let _ = app.reload(daemon_conn).await;
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let _ = app.volume_up(daemon_conn).await;
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            let _ = app.volume_down(daemon_conn).await;
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
            if modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+Tab: Previous tab
                app.prev_tab();
            } else {
                // Tab: Next tab
                app.next_tab();
            }
        }
        KeyCode::BackTab => {
            // Shift+Tab also goes to previous tab (for compatibility)
            app.prev_tab();
        }
        KeyCode::Char('[') => {
            // Load previous page from API
            if app.current_page > 1 {
                app.pending_page_change = Some(-1);
            } else {
                app.show_warning("Already on first page".to_string());
            }
        }
        KeyCode::Char(']') => {
            // Load next page from API
            tracing::info!("'] key pressed: current_page={}, is_last_page={}", app.current_page, app.is_last_page);
            app.add_log(format!("] pressed: page={}, is_last={}", app.current_page, app.is_last_page));
            if !app.is_last_page {
                tracing::info!("Setting pending_page_change to +1");
                app.pending_page_change = Some(1);
            } else {
                tracing::info!("Showing 'already on last page' warning");
                app.show_warning("Already on last page".to_string());
            }
        }
        _ => {}
    }
}
