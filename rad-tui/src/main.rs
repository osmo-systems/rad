mod app;
mod cli;
mod daemon;
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

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a == "--daemon") {
        // Daemon mode: single-threaded runtime + LocalSet for non-Send AudioPlayer
        return daemon::run();
    }

    // TUI / CLI mode: multi-threaded runtime
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run_tui_or_cli(args))
}

async fn run_tui_or_cli(args: Vec<String>) -> Result<()> {
    let data_dir = get_data_dir()?;

    let log_file = tracing_appender::rolling::daily(&data_dir, "rad.log");
    tracing_subscriber::fmt()
        .with_writer(log_file)
        .with_ansi(false)
        .init();

    if let Err(e) = cleanup_old_logs(&data_dir, 7) {
        tracing::warn!("Failed to clean up old logs: {}", e);
    }

    if args.len() > 1 {
        return cli::run(args, &data_dir).await;
    }

    info!("Starting Web Radio TUI");
    run_tui(data_dir).await
}

async fn run_tui(data_dir: std::path::PathBuf) -> Result<()> {
    // Connect to daemon (starts it if needed) and subscribe to push updates
    let daemon_client = PlayerDaemonClient::new()?;
    let mut subscription = match daemon_client.subscribe().await {
        Ok(sub) => {
            info!("Subscribed to player daemon");
            sub
        }
        Err(e) => {
            eprintln!("Failed to connect to player daemon: {}", e);
            return Err(e);
        }
    };

    let api_client = match tokio::time::timeout(
        std::time::Duration::from_secs(10),
        RadioBrowserClient::new(),
    )
    .await
    {
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

    let mut app = App::new(data_dir, api_client).await?;

    // Receive the initial state snapshot pushed by the daemon on subscribe
    if let Ok(Some(info)) = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        subscription.recv(),
    )
    .await
    {
        app.player_info = info;
        info!("Received initial player state from daemon");
    }

    // Calibrate the query limit to the actual terminal height before the first search.
    if let Ok(size) = terminal.size() {
        app.current_query.limit = (size.height.saturating_sub(11) as usize).max(1);
    }

    if let Err(e) = app.execute_search().await {
        tracing::error!("Failed to load initial stations: {}", e);
        app.status_message = Some(format!(
            "Failed to load stations: {}. Check network connection.",
            e
        ));
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

    if app.config.play_at_startup && !app.player_info.station_url.is_empty() {
        info!("play_at_startup: resuming last station");
        if let Err(e) = app.play_restored(&mut subscription).await {
            tracing::warn!("Failed to auto-play at startup: {}", e);
        }
    }

    app.add_log(tui_kit::LogLevel::Info, "TUI opened".to_string());

    let mut terminal = terminal;
    let mut tick_interval = interval(Duration::from_millis(100));

    loop {
        app.tick_toasts();
        app.animation_frame = (app.animation_frame + 1) % 48;

        terminal.draw(|f| ui::draw(f, &mut app))?;

        // Handle deferred search / page change
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
            } else if let Err(e) = app.prev_page().await {
                tracing::error!("Failed to load previous page: {}", e);
                app.show_error(format!("Failed to load page: {}", e));
            }
        }

        // Wait for the next event: state push, tick, key, or Ctrl+C
        let pending_key = tokio::select! {
            _ = tick_interval.tick() => None,

            _ = tokio::signal::ctrl_c() => {
                info!("Received Ctrl+C, shutting down...");
                app.quit();
                None
            }

            info = subscription.recv() => {
                if let Some(info) = info {
                    if let Some(ref err) = info.error_message {
                        if app.error_popup.is_none() {
                            app.show_error(err.clone());
                        }
                    }
                    app.player_info = info;
                }
                None
            }

            key = async {
                if event::poll(Duration::from_millis(50)).unwrap_or(false) {
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
            keys::handle_key_event(&mut app, &mut subscription, code, modifiers).await;
        }

        if !app.running {
            break;
        }
    }

    app.save_log();

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
