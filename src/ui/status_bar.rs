use ratatui::Frame;
use ratatui::layout::Rect;

use tui_kit::{render_footer_with_app, Theme};

use crate::app::{App, HelpTab};
use rad::PlayerState;

pub(super) fn draw_status_bar(f: &mut Frame, app: &App, area: Rect, theme: &Theme) {
    let mut pairs: Vec<(&str, &str)> = Vec::new();

    if app.error_popup.is_some() {
        pairs.push(("Esc/Enter", "close"));
        pairs.push(("Ctrl+C", "quit"));
    } else if app.warning_popup.is_some() {
        pairs.push(("Esc/Enter", "close"));
    } else if app.help_popup {
        match app.help_tab {
            HelpTab::Keys => {
                pairs.push(("Tab", "settings"));
                pairs.push(("Esc/?", "close"));
            }
            HelpTab::Settings => {
                pairs.push(("↑↓", "navigate"));
                pairs.push(("←→/Enter", "change"));
                pairs.push(("Tab", "log"));
                pairs.push(("Esc", "close"));
            }
            HelpTab::Log => {
                pairs.push(("↑↓", "scroll"));
                pairs.push(("f", "filter"));
                pairs.push(("Tab", "keys"));
                pairs.push(("Esc", "close"));
            }
        }
    } else if app.search_popup.is_some() {
        pairs.push(("Enter", "search"));
        pairs.push(("Tab", "complete"));
        pairs.push(("↑↓", "suggestions"));
        pairs.push(("Esc", "cancel"));
    } else {
        if app.stations.is_empty() {
            pairs.push(("/", "search"));
            pairs.push(("F1", "popular"));
        } else {
            pairs.push(("↑↓", "nav"));
            pairs.push(("Enter", "play"));

            match app.player_info.state {
                PlayerState::Playing | PlayerState::Paused => {
                    pairs.push(("Space", "pause"));
                    pairs.push(("s", "stop"));
                    pairs.push(("r", "reload"));
                }
                PlayerState::Loading => {
                    pairs.push(("s", "stop"));
                }
                _ => {}
            }

            pairs.push(("+-", "vol"));
            pairs.push(("f", "fav"));
            pairs.push(("v", "vote"));
            pairs.push(("V", "autovote"));
            pairs.push(("/", "search"));
            pairs.push(("np", "page"));
            pairs.push(("Tab", "tabs"));
        }
        pairs.push(("?", "help"));
        pairs.push(("Ctrl+C", "quit"));
    }

    render_footer_with_app(f, area, &pairs, "rad", env!("CARGO_PKG_VERSION"), theme);
}
