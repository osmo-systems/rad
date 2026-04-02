use crossterm::event::{KeyCode, KeyModifiers};
use tui_kit::LogLevel;

use crate::app::{App, ConfirmDelete, HelpTab, Tab};
use rad::{
    config::TOAST_DURATION_OPTIONS,
    ipc::ClientMessage,
    search::{get_suggestions, parse_query},
    DaemonSubscription,
};

pub async fn handle_key_event(
    app: &mut App,
    daemon: &mut DaemonSubscription,
    key: KeyCode,
    modifiers: KeyModifiers,
) {
    tracing::info!("handle_key_event called with key: {:?}, modifiers: {:?}", key, modifiers);

    if modifiers.contains(KeyModifiers::CONTROL) && matches!(key, KeyCode::Char('c')) {
        app.quit();
        return;
    }

    // Help popup
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
                KeyCode::Char('f') => {
                    app.log_level_filter = match app.log_level_filter {
                        None => Some(LogLevel::Error),
                        Some(LogLevel::Error) => Some(LogLevel::Warning),
                        Some(LogLevel::Warning) => Some(LogLevel::Info),
                        Some(LogLevel::Info) => Some(LogLevel::Debug),
                        Some(LogLevel::Debug) => None,
                    };
                    app.help_log_scroll = 0;
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
                    if app.settings_selected < 4 {
                        app.settings_selected += 1;
                    }
                }
                KeyCode::Right | KeyCode::Enter => match app.settings_selected {
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
                        app.config.autovote_enabled = !app.config.autovote_enabled;
                        let _ = app.config.save(&app.data_dir);
                        if !app.config.autovote_enabled && app.current_tab == Tab::Autovote {
                            app.current_tab = Tab::Favorites;
                            app.reload_current_tab();
                        }
                    }
                    4 => {
                        app.config.show_logo = !app.config.show_logo;
                        let _ = app.config.save(&app.data_dir);
                    }
                    _ => {
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
                },
                KeyCode::Left => match app.settings_selected {
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
                        app.config.autovote_enabled = !app.config.autovote_enabled;
                        let _ = app.config.save(&app.data_dir);
                        if !app.config.autovote_enabled && app.current_tab == Tab::Autovote {
                            app.current_tab = Tab::Favorites;
                            app.reload_current_tab();
                        }
                    }
                    4 => {
                        app.config.show_logo = !app.config.show_logo;
                        let _ = app.config.save(&app.data_dir);
                    }
                    _ => {
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
                },
                _ => {}
            },
        }
        return;
    }

    // Error/warning popup
    if app.error_popup.is_some() || app.warning_popup.is_some() {
        tracing::info!("Error/warning popup is open, key pressed: {:?}", key);
        match key {
            KeyCode::Esc | KeyCode::Enter => {
                tracing::info!("Closing error/warning popup");
                app.close_error_popup();
                let _ = daemon.send_command(ClientMessage::ClearError).await;
                app.player_info.error_message = None;
            }
            _ => {
                tracing::info!("Ignoring key: {:?}", key);
            }
        }
        return;
    }

    // Confirm delete popup
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

    // Search popup
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
                    let query_str = popup.get_query();
                    tracing::info!("Enter pressed in popup, query: '{}'", query_str);
                    match parse_query(query_str) {
                        Ok(query) => {
                            tracing::info!("Query parsed successfully: {:?}", query);
                            app.current_query = query;
                            app.close_search_popup();
                            tracing::info!("Popup closed, triggering pending_search");
                            app.pending_search = true;
                        }
                        Err(e) => {
                            tracing::error!("Query parse error: {:?}", e);
                            app.show_error(format!("Invalid query: {}", e));
                        }
                    }
                }
            }
            KeyCode::Esc => {
                if let Some(popup) = &mut app.search_popup {
                    if popup.autocomplete_shown {
                        popup.autocomplete_shown = false;
                    } else {
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
            app.page_up();
        }
        KeyCode::PageDown => {
            app.page_down();
        }
        KeyCode::Home => {
            app.selected_index = 0;
            app.scroll_offset = 0;
        }
        KeyCode::End => {
            if !app.stations.is_empty() {
                app.selected_index = app.stations.len() - 1;
            }
        }
        KeyCode::Enter => {
            if let Err(e) = app.play_selected(daemon).await {
                tracing::error!("Failed to play station: {}", e);
                app.show_error(format!("Failed to play station: {}", e));
            }
        }
        KeyCode::Char(' ') => {
            if app.player_info.state == rad::PlayerState::Playing {
                let _ = app.pause(daemon).await;
            } else if app.player_info.state == rad::PlayerState::Paused {
                let _ = app.resume(daemon).await;
            } else if app.player_info.state == rad::PlayerState::Stopped && !app.player_info.station_url.is_empty() {
                let _ = app.play_restored(daemon).await;
            } else {
                let _ = app.resume(daemon).await;
            }
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            let _ = app.stop(daemon).await;
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            let _ = app.reload(daemon).await;
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let _ = app.volume_up(daemon).await;
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            let _ = app.volume_down(daemon).await;
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            let selected_uuid = app.get_selected_station()
                .map(|s| s.station_uuid.clone());
            if let Err(e) = app.toggle_favorite().await {
                tracing::error!("Failed to toggle favorite: {}", e);
            } else if app.config.autovote_enabled {
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
            if app.config.autovote_enabled && app.current_tab != Tab::Autovote {
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
            if let Err(e) = app.first_page().await {
                tracing::error!("Failed to reload stations: {}", e);
                app.show_error(format!("Failed to reload stations: {}", e));
            }
        }
        KeyCode::Tab => {
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
