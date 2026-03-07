//! Interactive and non-interactive search functionality for CLI using cliclack

use anyhow::Result;
use cliclack::select;
use rad_core::{
    api::Station,
    search::SearchQuery,
};

/// Enum representing user actions in the interactive search
#[derive(Debug, Clone)]
pub enum SearchAction {
    Play(String, String), // (name, url)
    NextPage,
    PrevPage,
}

/// Run interactive search mode with multi-select capability
pub async fn run_interactive_search_with_select(
    results: Vec<Station>,
    query: &SearchQuery,
) -> Result<Option<SearchAction>> {
    if results.is_empty() {
        return Ok(None);
    }
    
    // Build select with individual items instead of .items() to avoid rendering issues
    let mut selection = select("Select a station to play");
    
    // Add station items
    for (i, station) in results.iter().enumerate() {
        let label = station.name.clone();
        let hint = format!("{} • {}", station.country, station.language);
        selection = selection.item(i, label, hint);
    }
    
    // Add pagination options
    let next_idx = results.len();
    let mut prev_idx = results.len() + 1;
    
    let has_more = results.len() == query.limit;
    let has_prev = query.offset > 0;
    
    if has_more {
        selection = selection.item(next_idx, "→ See next stations", "Load more results");
    }
    
    if has_prev {
        if !has_more {
            prev_idx = next_idx;
        }
        selection = selection.item(prev_idx, "← See previous stations", "Go back");
    }
    
    let selected_idx = selection.interact()?;

    // Handle selection
    if selected_idx < results.len() {
        // Station selected
        Ok(Some(SearchAction::Play(
            results[selected_idx].name.clone(),
            results[selected_idx].url_resolved.clone(),
        )))
    } else if has_more && selected_idx == next_idx {
        Ok(Some(SearchAction::NextPage))
    } else if has_prev && selected_idx == prev_idx {
        Ok(Some(SearchAction::PrevPage))
    } else {
        Ok(None)
    }
}

/// Parse CLI arguments into SearchQuery
pub fn parse_search_args(args: &[String]) -> SearchQuery {
    let mut query = SearchQuery::default();
    let mut i = 2; // Skip program name and "find" command

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
                match args[i + 1].parse() {
                    Ok(val) => query.bitrate_min = Some(val),
                    Err(_) => {
                        eprintln!("Warning: Invalid bitrate-min value '{}', ignoring", args[i + 1]);
                    }
                }
                i += 2;
            }
            "--bitrate-max" if i + 1 < args.len() => {
                match args[i + 1].parse() {
                    Ok(val) => query.bitrate_max = Some(val),
                    Err(_) => {
                        eprintln!("Warning: Invalid bitrate-max value '{}', ignoring", args[i + 1]);
                    }
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
                match args[i + 1].parse() {
                    Ok(val) => query.limit = val,
                    Err(_) => {
                        eprintln!("Warning: Invalid limit value '{}', using default", args[i + 1]);
                    }
                }
                i += 2;
            }
            "--skip" if i + 1 < args.len() => {
                match args[i + 1].parse() {
                    Ok(val) => query.offset = val,
                    Err(_) => {
                        eprintln!("Warning: Invalid skip value '{}', ignoring", args[i + 1]);
                    }
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
