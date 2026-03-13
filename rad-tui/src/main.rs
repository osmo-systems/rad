mod app;
mod cli;
mod keys;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, time::Duration};
use tokio::time::interval;
use tracing::info;
use tracing_subscriber;

use crate::app::{App, Tab};
use rad_core::{
    config::{cleanup_old_logs, get_data_dir},
    PlayerDaemonClient,
    RadioBrowserClient,
};

#[tokio::main]
async fn main() -> Result<()> {
    let data_dir = get_data_dir()?;

    let log_file = tracing_appender::rolling::daily(&data_dir, "rad.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    if let Err(e) = cleanup_old_logs(&data_dir, 7) {
        tracing::warn!("Failed to clean up old logs: {}", e);
    }

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        return cli::run(args, &data_dir).await;
    }

    info!("Starting Web Radio TUI");

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

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;

    let mut app = App::new(data_dir, api_client, &mut daemon_conn).await?;

    if let Ok(player_info) = daemon_conn.get_status().await {
        app.player_info = player_info;
        info!("Retrieved initial player status from daemon");
    }

    // Calibrate the query limit to the actual terminal height before the first search.
    // Layout overhead: 8 (player+log) + 1 (status bar) + 2 (list borders) = 11 rows.
    if let Ok(size) = terminal.size() {
        app.current_query.limit = (size.height.saturating_sub(11) as usize).max(1);
    }

    if let Err(e) = app.execute_search().await {
        tracing::error!("Failed to load initial stations: {}", e);
        app.status_message = Some(format!("Failed to load stations: {}. Check network connection.", e));
    }

    if app.current_tab != Tab::Browse {
        app.reload_current_tab();
    }

    tracing::info!("Initial data loaded. Stations count: {}", app.stations.len());

    if let Err(e) = app.auto_vote_favorites().await {
        tracing::warn!("Auto-vote for favorites failed: {}", e);
    }

    if let Err(e) = app.auto_vote_autovote_list().await {
        tracing::warn!("Auto-vote list failed: {}", e);
    }

    if app.config.play_at_startup {
        if !app.player_info.station_url.is_empty() {
            info!("play_at_startup: resuming last station");
            if let Err(e) = app.play_restored(&mut daemon_conn).await {
                tracing::warn!("Failed to auto-play at startup: {}", e);
            }
        }
    }

    let mut terminal = terminal;
    let mut tick_interval = interval(Duration::from_millis(100));

    loop {
        if let Ok(player_info) = daemon_conn.get_status().await {
            app.player_info = player_info;
        }

        if let Some(ref err_msg) = app.player_info.error_message {
            if app.error_popup.is_none() {
                app.show_error(err_msg.clone());
            }
        }

        app.tick_toasts();
        app.animation_frame = (app.animation_frame + 1) % 48;

        terminal.draw(|f| ui::draw(f, &mut app))?;

        if app.pending_search {
            app.pending_search = false;
            tracing::info!("Executing pending search");
            if let Err(e) = app.execute_search().await {
                tracing::error!("Search failed: {}", e);
                app.show_error(format!("Search failed: {}", e));
            }
        }

        if let Some(direction) = app.pending_page_change {
            app.pending_page_change = None;
            tracing::info!("Executing pending page change: direction={}", direction);
            if direction > 0 {
                if let Err(e) = app.next_page().await {
                    tracing::error!("Failed to load next page: {}", e);
                    app.show_error(format!("Failed to load page: {}", e));
                }
            } else {
                if let Err(e) = app.prev_page().await {
                    tracing::error!("Failed to load previous page: {}", e);
                    app.show_error(format!("Failed to load page: {}", e));
                }
            }
        }

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
            keys::handle_key_event(&mut app, &mut daemon_conn, code, modifiers).await;
        }

        if !app.running {
            break;
        }
    }

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
