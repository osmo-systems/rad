mod app;
mod cli;
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

use crate::app::{App, ConfirmDelete, HelpTab, Tab};
use rad_core::{
    config::{cleanup_old_logs, get_data_dir, TOAST_DURATION_OPTIONS},
    search::{get_suggestions, parse_query},
    PlayerDaemonClient,
    RadioBrowserClient,
};

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = get_data_dir()?;

    // Shared log file for both TUI and CLI modes
    let log_file = tracing_appender::rolling::daily(&data_dir, "rad.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    if let Err(e) = cleanup_old_logs(&data_dir, 7) {
        tracing::warn!("Failed to clean up old logs: {}", e);
    }

    // If any arguments are provided, run in CLI mode
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        return cli::run(args, &data_dir).await;
    }

    info!("Starting Web Radio TUI");

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

    // Calibrate the query limit to the actual terminal height before the first search.
    // Layout overhead: 8 (player+log) + 1 (status bar) + 2 (list borders) = 11 rows.
    if let Ok(size) = terminal.size() {
        app.current_query.limit = (size.height.saturating_sub(11) as usize).max(1);
    }

    // Load initial data (popular stations with default query)
    if let Err(e) = app.execute_search().await {
        tracing::error!("Failed to load initial stations: {}", e);
        app.status_message = Some(format!("Failed to load stations: {}. Check network connection.", e));
    }

    // execute_search always loads Browse content; switch to the configured startup tab if needed
    if app.current_tab != Tab::Browse {
        app.reload_current_tab();
    }

    tracing::info!("Initial data loaded. Stations count: {}", app.stations.len());

    // Auto-vote favorites at startup if configured (legacy)
    if let Err(e) = app.auto_vote_favorites().await {
        tracing::warn!("Auto-vote for favorites failed: {}", e);
    }

    // Auto-vote dedicated autovote list at startup
    if let Err(e) = app.auto_vote_autovote_list().await {
        tracing::warn!("Auto-vote list failed: {}", e);
    }

    // Auto-play last station at startup if configured
    if app.config.play_at_startup {
        if !app.player_info.station_url.is_empty() {
            info!("play_at_startup: resuming last station");
            if let Err(e) = app.play_restored(&mut daemon_conn).await {
                tracing::warn!("Failed to auto-play at startup: {}", e);
            }
        }
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

        // Expire toasts
        app.tick_toasts();

        // Update animation frame
        app.animation_frame = (app.animation_frame + 1) % 48;

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

        // Read the next event without doing any async work inside select!.
        // handle_key_event is called OUTSIDE the select so long-running operations
        // (e.g. network calls for voting) are never cancelled by the tick branch.
        let pending_key = tokio::select! {
            _ = tick_interval.tick() => None,
            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C signal, shutting down...");
                app.quit();
                None
            }
            key = async {
                if event::poll(Duration::from_millis(50)).unwrap() {
                    if let Ok(Event::Key(key)) = event::read() {
                        if key.kind == KeyEventKind::Press {
                            return Some((key.code, key.modifiers));
                        }
                    }
                }
                None
            } => key,
        };

        if let Some((code, modifiers)) = pending_key {
            handle_key_event(&mut app, &mut daemon_conn, code, modifiers).await;
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
    daemon_conn: &mut rad_core::PlayerDaemonConnection,
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
        match app.help_tab {
            HelpTab::Keys => match key {
                KeyCode::Esc | KeyCode::Char('?') => {
                    app.help_popup = false;
                }
                KeyCode::Tab => {
                    app.help_tab = HelpTab::Settings;
                }
                _ => {}
            },
            HelpTab::Log => match key {
                KeyCode::Esc | KeyCode::Char('?') => {
                    app.help_popup = false;
                }
                KeyCode::Tab => {
                    app.help_tab = HelpTab::Keys;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if app.help_log_scroll > 0 {
                        app.help_log_scroll -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    let count = app.status_log.len();
                    if count > 0 && app.help_log_scroll < count - 1 {
                        app.help_log_scroll += 1;
                    }
                }
                _ => {}
            },
            HelpTab::Settings => match key {
                KeyCode::Esc | KeyCode::Char('?') => {
                    app.help_popup = false;
                }
                KeyCode::Tab => {
                    app.help_tab = HelpTab::Log;
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    if app.settings_selected > 0 {
                        app.settings_selected -= 1;
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if app.settings_selected < 5 {
                        app.settings_selected += 1;
                    }
                }
                KeyCode::Right | KeyCode::Enter => {
                    match app.settings_selected {
                        0 => {
                            app.config.startup_tab = app.config.startup_tab.cycle_next();
                            let _ = app.config.save(&app.data_dir);
                        }
                        1 => {
                            app.config.default_search_order = app.config.default_search_order.cycle_next();
                            app.current_query.order = Some(app.config.default_search_order.as_api_str().to_string());
                            let _ = app.config.save(&app.data_dir);
                        }
                        2 => {
                            app.config.play_at_startup = !app.config.play_at_startup;
                            let _ = app.config.save(&app.data_dir);
                        }
                        3 => {
                            app.config.auto_vote_favorites = !app.config.auto_vote_favorites;
                            let _ = app.config.save(&app.data_dir);
                            // If autovote was just disabled, leave the Autovote tab
                            if !app.config.auto_vote_favorites && app.current_tab == Tab::Autovote {
                                app.current_tab = Tab::Favorites;
                                app.reload_current_tab();
                            }
                        }
                        4 => {
                            app.config.show_logo = !app.config.show_logo;
                            let _ = app.config.save(&app.data_dir);
                        }
                        5 => {
                            let cur = app.config.toast_duration_secs;
                            let next = TOAST_DURATION_OPTIONS
                                .iter()
                                .skip_while(|&&v| v != cur)
                                .nth(1)
                                .copied()
                                .unwrap_or(TOAST_DURATION_OPTIONS[0]);
                            app.config.toast_duration_secs = next;
                            let _ = app.config.save(&app.data_dir);
                        }
                        _ => {}
                    }
                }
                KeyCode::Left => {
                    match app.settings_selected {
                        0 => {
                            app.config.startup_tab = app.config.startup_tab.cycle_prev();
                            let _ = app.config.save(&app.data_dir);
                        }
                        1 => {
                            app.config.default_search_order = app.config.default_search_order.cycle_prev();
                            app.current_query.order = Some(app.config.default_search_order.as_api_str().to_string());
                            let _ = app.config.save(&app.data_dir);
                        }
                        2 => {
                            app.config.play_at_startup = !app.config.play_at_startup;
                            let _ = app.config.save(&app.data_dir);
                        }
                        3 => {
                            app.config.auto_vote_favorites = !app.config.auto_vote_favorites;
                            let _ = app.config.save(&app.data_dir);
                            if !app.config.auto_vote_favorites && app.current_tab == Tab::Autovote {
                                app.current_tab = Tab::Favorites;
                                app.reload_current_tab();
                            }
                        }
                        4 => {
                            app.config.show_logo = !app.config.show_logo;
                            let _ = app.config.save(&app.data_dir);
                        }
                        5 => {
                            let cur = app.config.toast_duration_secs;
                            let prev = TOAST_DURATION_OPTIONS
                                .iter()
                                .rev()
                                .skip_while(|&&v| v != cur)
                                .nth(1)
                                .copied()
                                .unwrap_or(TOAST_DURATION_OPTIONS[TOAST_DURATION_OPTIONS.len() - 1]);
                            app.config.toast_duration_secs = prev;
                            let _ = app.config.save(&app.data_dir);
                        }
                        _ => {}
                    }
                }
                _ => {}
            },
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
                // Clear error on daemon side so get_status() stops returning it,
                // and clear it locally so the re-trigger check doesn't fire this tick.
                let _ = daemon_conn.clear_error().await;
                app.player_info.error_message = None;
            }
            _ => {
                tracing::info!("Ignoring key: {:?}", key);
            }
        }
        return;
    }

    // Handle confirm delete popup
    if app.confirm_delete.is_some() {
        match key {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                match app.confirm_delete.take() {
                    Some(ConfirmDelete::Favorite(uuid, name)) => {
                        if let Err(e) = app.favorites.remove(&uuid) {
                            tracing::error!("Failed to remove favorite: {}", e);
                        } else {
                            app.show_toast(format!("Removed {} from favorites", name), tui_kit::ToastLevel::Warning);
                            app.reload_current_tab();
                            if app.selected_index > 0 && app.selected_index >= app.stations.len() {
                                app.selected_index = app.stations.len().saturating_sub(1);
                            }
                        }
                    }
                    Some(ConfirmDelete::Autovote(uuid, name)) => {
                        if let Err(e) = app.autovote.remove(&uuid) {
                            tracing::error!("Failed to remove from autovote: {}", e);
                        } else {
                            app.show_toast(format!("Removed {} from autovote", name), tui_kit::ToastLevel::Warning);
                            let count = app.autovote.get_all().len();
                            if count == 0 {
                                app.current_tab = Tab::Favorites;
                                app.reload_current_tab();
                                app.autovote_selected = 0;
                            } else if app.autovote_selected >= count {
                                app.autovote_selected = count - 1;
                            }
                        }
                    }
                    None => {}
                }
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                app.confirm_delete = None;
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
                    popup.delete_word();
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
            app.help_tab = HelpTab::Keys;
            app.settings_selected = 0;
        }
        KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
        KeyCode::Up => {
            if app.current_tab == Tab::Autovote {
                if app.autovote_selected > 0 {
                    app.autovote_selected -= 1;
                }
            } else {
                app.select_prev();
            }
        }
        KeyCode::Down => {
            if app.current_tab == Tab::Autovote {
                let count = app.autovote.get_all().len();
                if app.autovote_selected + 1 < count {
                    app.autovote_selected += 1;
                }
            } else {
                app.select_next();
            }
        }
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
            if app.player_info.state == rad_core::PlayerState::Playing {
                let _ = app.pause(daemon_conn).await;
            } else if app.player_info.state == rad_core::PlayerState::Paused {
                let _ = app.resume(daemon_conn).await;
            } else if app.player_info.state == rad_core::PlayerState::Stopped && !app.player_info.station_url.is_empty() {
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
            // Capture UUID before the mutable borrow for toggle
            let selected_uuid = app.get_selected_station()
                .map(|s| s.station_uuid.clone());
            if let Err(e) = app.toggle_favorite().await {
                tracing::error!("Failed to toggle favorite: {}", e);
            } else if app.config.auto_vote_favorites {
                // If the station was just added to favorites, vote for it immediately
                if let Some(uuid) = selected_uuid {
                    if app.favorites.is_favorite(&uuid)
                        && !app.vote_manager.has_voted_recently(&uuid)
                    {
                        match app.api_client.vote_for_station(&uuid).await {
                            Ok(_) => { let _ = app.vote_manager.record_vote(&uuid); }
                            Err(e) => tracing::warn!("Auto-vote on favorite add failed: {}", e),
                        }
                    }
                }
            }
        }
        KeyCode::Char('v') => {
            if let Err(e) = app.vote_for_selected().await {
                tracing::error!("Failed to vote: {}", e);
            }
        }
        KeyCode::Char('V') => {
            app.toggle_autovote();
        }
        KeyCode::Char('1') => {
            if app.current_tab != Tab::Browse {
                if matches!(app.current_tab, Tab::Browse) { app.browse_stations = app.stations.clone(); }
                app.current_tab = Tab::Browse;
                app.reload_current_tab();
            }
        }
        KeyCode::Char('2') => {
            if app.current_tab != Tab::Favorites {
                if matches!(app.current_tab, Tab::Browse) { app.browse_stations = app.stations.clone(); }
                app.current_tab = Tab::Favorites;
                app.reload_current_tab();
            }
        }
        KeyCode::Char('3') => {
            if app.current_tab != Tab::History {
                if matches!(app.current_tab, Tab::Browse) { app.browse_stations = app.stations.clone(); }
                app.current_tab = Tab::History;
                app.reload_current_tab();
            }
        }
        KeyCode::Char('4') => {
            if app.config.auto_vote_favorites && app.current_tab != Tab::Autovote {
                if matches!(app.current_tab, Tab::Browse) { app.browse_stations = app.stations.clone(); }
                app.current_tab = Tab::Autovote;
                app.autovote_selected = 0;
                app.reload_current_tab();
            }
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            if app.current_tab == Tab::Autovote {
                if let Some(s) = app.autovote.get_all().get(app.autovote_selected) {
                    app.confirm_delete = Some(ConfirmDelete::Autovote(s.uuid.clone(), s.name.clone()));
                }
            } else if app.current_tab == Tab::Favorites {
                if let Some(s) = app.get_selected_station() {
                    app.confirm_delete = Some(ConfirmDelete::Favorite(s.station_uuid.clone(), s.name.clone()));
                }
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
            // Cycle through station list tabs
            app.next_tab();
        }
        KeyCode::BackTab => {
            app.prev_tab();
        }
        KeyCode::Char('n') => {
            if app.current_tab != Tab::Browse {
                app.show_warning("No pagination on this tab".to_string());
            } else if !app.is_last_page {
                tracing::info!("'n' pressed: current_page={}", app.current_page);
                app.pending_page_change = Some(1);
            } else {
                app.show_warning("Already on last page".to_string());
            }
        }
        KeyCode::Char('p') => {
            if app.current_tab != Tab::Browse {
                app.show_warning("No pagination on this tab".to_string());
            } else if app.current_page > 1 {
                app.pending_page_change = Some(-1);
            } else {
                app.show_warning("Already on first page".to_string());
            }
        }
        _ => {}
    }
}
