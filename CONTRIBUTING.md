# Contributing to LazyRadio

Thank you for your interest in contributing to LazyRadio! This document provides guidelines and information for contributors.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [How Can I Contribute?](#how-can-i-contribute)
  - [Reporting Bugs](#reporting-bugs)
  - [Suggesting Features](#suggesting-features)
  - [Contributing Code](#contributing-code)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Code Guidelines](#code-guidelines)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)

## Code of Conduct

This project adheres to a simple code of conduct: be respectful, constructive, and professional in all interactions. We're here to build great software and help each other learn.

## How Can I Contribute?

### Reporting Bugs

Before creating a bug report, please check existing issues to avoid duplicates. When creating a bug report, include:

- **Clear title**: Summarize the issue in one line
- **Description**: Detailed explanation of the problem
- **Steps to reproduce**: Exact steps to trigger the bug
- **Expected behavior**: What should happen
- **Actual behavior**: What actually happens
- **Environment**: OS, Rust version, terminal emulator
- **Logs**: Relevant log entries from `~/.local/share/lazyradio/lazyradio.log`

Use the bug report template in `.github/ISSUE_TEMPLATE/bug_report.md`.

### Suggesting Features

Feature suggestions are welcome! Before submitting:

1. **Check existing issues**: Someone may have suggested it already
2. **Consider scope**: Does it align with LazyRadio's goals?
3. **Think about users**: How would this benefit most users?

When suggesting a feature, include:

- **Use case**: Why is this feature needed?
- **Proposed solution**: How should it work?
- **Alternatives**: What other approaches did you consider?
- **Additional context**: Screenshots, mockups, examples

Use the feature request template in `.github/ISSUE_TEMPLATE/feature_request.md`.

### Contributing Code

We welcome code contributions! Here's how to get started:

1. **Fork the repository** on GitHub
2. **Clone your fork**:
   ```bash
   git clone https://github.com/YOUR_USERNAME/lazyradio.git
   cd lazyradio
   ```
3. **Create a branch**:
   ```bash
   git checkout -b feature/your-feature-name
   ```
4. **Make your changes** (see Development Setup below)
5. **Test your changes** thoroughly
6. **Commit with clear messages**:
   ```bash
   git commit -m "Add feature: your feature description"
   ```
7. **Push to your fork**:
   ```bash
   git push origin feature/your-feature-name
   ```
8. **Create a Pull Request** on GitHub

## Development Setup

### Prerequisites

- **Rust 1.83 or later**: Install via [rustup](https://rustup.rs/)
- **ALSA development libraries** (Linux only):
  - Fedora/RHEL: `sudo dnf install alsa-lib-devel`
  - Debian/Ubuntu: `sudo apt-get install libasound2-dev`
  - Arch: `sudo pacman -S alsa-lib`

### Building

```bash
# Clone the repository
git clone https://github.com/osmo-systems/lazyradio.git
cd lazyradio

# Build in debug mode (faster compilation, slower runtime)
cargo build

# Build in release mode (slower compilation, faster runtime)
cargo build --release

# Run in debug mode
cargo run

# Run in release mode
cargo run --release
```

### Development Tools

```bash
# Run with debug logging to terminal
RUST_LOG=debug cargo run

# Run with trace-level logging (very verbose)
RUST_LOG=trace cargo run

# Run tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Check code without building
cargo check

# Format code
cargo fmt

# Lint code
cargo clippy

# Run clippy with strict settings
cargo clippy -- -D warnings
```

### Log Files

When developing, logs are written to:
- **Linux**: `~/.local/share/lazyradio/lazyradio.log`
- **macOS**: `~/Library/Application Support/lazyradio/lazyradio.log`
- **Windows**: `%APPDATA%\lazyradio\lazyradio.log`

Use `tail -f` to monitor logs in real-time:
```bash
tail -f ~/.local/share/lazyradio/lazyradio.log
```

## Project Structure

Understanding the codebase layout:

```
src/
├── main.rs              # Entry point: event loop and terminal setup
├── app.rs               # Application state: tabs, selections, player state
├── config.rs            # Configuration loading and defaults
│
├── api/                 # Radio Browser API integration
│   ├── client.rs        # HTTP client with DNS-based server discovery
│   └── models.rs        # Station data structures (serde models)
│
├── player/              # Audio playback system
│   └── audio.rs         # Rodio-based streaming audio player
│
├── storage/             # Persistent data management
│   ├── favorites.rs     # Favorite stations (TOML)
│   ├── history.rs       # Play history (TOML, limited to 50 entries)
│   ├── cache.rs         # API response caching (file-based)
│   └── search_history.rs # Search query history (TOML, last 50)
│
├── search/              # Advanced search system
│   ├── parser.rs        # Query parser (field=value syntax)
│   ├── autocomplete.rs  # Autocomplete engine (13 field types)
│   └── mod.rs           # Public API and coordination
│
└── ui/                  # Terminal user interface
    ├── layout.rs        # Main UI layout (ratatui)
    └── search_popup.rs  # Search popup with autocomplete widget
```

### Key Modules

#### `main.rs` - Event Loop
- Terminal initialization (raw mode, alternate screen)
- Event handling (keyboard, terminal resize)
- Calls `app.update()` and `ui::render()`

#### `app.rs` - State Management
- Three tabs: Browse, Favorites, History
- Station lists and selection state
- Player state (playing, paused, stopped)
- Communication with player thread via channels

#### `api/client.rs` - Radio Browser Client
- DNS-based server discovery (`all.api.radio-browser.info`)
- Load balancing across multiple API servers
- Station search with query parameters
- Voting and click tracking

#### `player/audio.rs` - Audio Player
- Runs in separate thread to avoid blocking UI
- Streams audio using `stream-download` + `rodio`
- Volume control and playback state management
- Error handling and reconnection logic

#### `search/` - Search System
- **parser.rs**: Parses `field=value` queries into structured data
- **autocomplete.rs**: Provides context-aware suggestions
- Supports 13 fields: name, country, countrycode, state, language, tag, codec, bitrate_min/max, order, reverse, hidebroken, is_https

#### `ui/` - Terminal UI
- **layout.rs**: Main layout with tabs, station list, player panel, status bar
- **search_popup.rs**: Modal search dialog with autocomplete dropdown
- Uses `ratatui` for rendering and `crossterm` for terminal control

## Code Guidelines

### Rust Style

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Run `cargo fmt` before committing (formats code automatically)
- Run `cargo clippy` and fix warnings
- Use meaningful variable and function names
- Add doc comments (`///`) for public APIs

### Error Handling

- Use `anyhow::Result` for application errors
- Use specific error messages with context
- Log errors at appropriate levels (error, warn, info, debug, trace)
- Display user-friendly error messages in the UI

Example:
```rust
use anyhow::{Context, Result};

fn load_favorites() -> Result<Vec<Station>> {
    let path = get_favorites_path()
        .context("Failed to determine favorites path")?;
    
    let content = std::fs::read_to_string(&path)
        .context(format!("Failed to read favorites from {}", path.display()))?;
    
    toml::from_str(&content)
        .context("Failed to parse favorites file")
}
```

### Logging

Use appropriate log levels:
- `error!`: Critical errors requiring attention
- `warn!`: Concerning but non-critical issues
- `info!`: High-level informational messages
- `debug!`: Detailed debugging information
- `trace!`: Very verbose debugging (e.g., every event)

Example:
```rust
use tracing::{info, debug, error};

info!("Starting audio playback for station: {}", station.name);
debug!("Stream URL: {}", station.url);

if let Err(e) = player.play(&url) {
    error!("Failed to play station: {:#}", e);
}
```

### Async Code

- Use `tokio` for async operations (API calls, file I/O)
- Keep UI thread synchronous (ratatui is not async)
- Use channels (`tokio::sync::mpsc`) for thread communication
- Handle cancellation gracefully

### UI Code

- Keep rendering logic in `ui/` module
- Separate state management (in `app.rs`) from rendering
- Use ratatui's builder patterns for widgets
- Test UI with different terminal sizes

## Testing

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_name

# Run tests for specific module
cargo test api::
```

### Writing Tests

Add unit tests in the same file:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_query() {
        let query = "name=jazz country=usa";
        let parsed = parse_search_query(query).unwrap();
        
        assert_eq!(parsed.name, Some("jazz".to_string()));
        assert_eq!(parsed.country, Some("usa".to_string()));
    }
}
```

Integration tests go in `tests/` directory.

### Manual Testing Checklist

Before submitting a PR, test:

- [ ] Basic playback (play, pause, stop, reload)
- [ ] Volume control (up/down)
- [ ] Tab navigation (Browse, Favorites, History)
- [ ] Search with various queries
- [ ] Autocomplete suggestions
- [ ] Adding/removing favorites
- [ ] Voting for stations
- [ ] Pagination (N/P keys)
- [ ] Error handling (offline stations, network issues)
- [ ] Terminal resize
- [ ] Logs contain no unexpected errors

## Pull Request Process

1. **Update documentation**: If you changed user-facing behavior, update README.md
2. **Add tests**: Include tests for new functionality
3. **Run checks**: Ensure `cargo test`, `cargo fmt`, and `cargo clippy` pass
4. **Write clear commit messages**: Explain what and why, not how
5. **Keep PRs focused**: One feature/fix per PR when possible
6. **Fill out PR template**: Use `.github/PULL_REQUEST_TEMPLATE.md`
7. **Be responsive**: Address review feedback promptly

### Commit Messages

Use clear, descriptive commit messages:

```
Good:
- "Add pagination support for search results"
- "Fix race condition in player thread"
- "Update README with advanced search examples"

Bad:
- "Fix bug"
- "Update stuff"
- "WIP"
```

Format:
```
Short summary (50 chars or less)

More detailed explanation if needed. Wrap at 72 characters.
Explain the problem this commit solves and why you solved it
this way.

Closes #123
```

### Code Review

All PRs require review before merging. Reviewers will check:

- Code quality and style
- Test coverage
- Documentation updates
- Performance implications
- Breaking changes

Be patient and respectful during review. Feedback is meant to improve the code, not criticize you personally.

## Questions?

If you have questions not covered here:

1. Check existing [GitHub Issues](https://github.com/osmo-systems/lazyradio/issues)
2. Search [Discussions](https://github.com/osmo-systems/lazyradio/discussions) (if enabled)
3. Open a new issue with the "question" template

## Thank You!

Your contributions make LazyRadio better for everyone. Whether you're fixing a typo, reporting a bug, or implementing a major feature - thank you for being part of this project!

---

**Happy coding!** 🎵🦀
