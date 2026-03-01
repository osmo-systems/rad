# LazyRadio

[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Rust 1.83+](https://img.shields.io/badge/rust-1.83%2B-orange.svg)](https://www.rust-lang.org)
[![Platform: Linux | macOS | Windows](https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey.svg)](https://github.com/osmo-systems/lazyradio)

A lightning-fast terminal-based web radio player with advanced search capabilities, powered by the [Radio Browser](https://www.radio-browser.info/) community database.

**Powered by Radio Browser** - LazyRadio is built on top of the free, open-source [Radio Browser API](https://www.radio-browser.info/), a community-driven database of 40,000+ radio stations from around the world. By using LazyRadio, you're supporting this amazing project through station votes and play tracking.

## Features

- 🎵 **Access 40,000+ Radio Stations** - Browse the entire Radio Browser community database
- 🔍 **Advanced Search with Autocomplete** - Field-based query syntax with intelligent suggestions (name, country, language, codec, bitrate, and more)
- ⚡ **Lightning Fast** - Built with Rust and optimized for performance with smart caching
- ⭐ **Favorites** - Save and organize your favorite stations locally
- 📜 **History** - Track recently played stations with automatic saving
- 🎛️ **Full Player Controls** - Play, pause, stop, reload, volume control
- 👍 **Support Radio Browser** - Vote for stations to improve the community database
- 💾 **Smart Caching** - Reduce API load with intelligent result caching
- 🖥️ **Cross-Platform** - Works seamlessly on Linux, macOS, and Windows
- 🔐 **Privacy-Focused** - All data stored locally, no telemetry

## Screenshots

```
┌Tabs────────────────────────────────────────────────────────────────┐
│ Browse  Favorites  History                                         │
└────────────────────────────────────────────────────────────────────┘
┌Stations (100 stations)─────────────────────────────────────────────┐
│ ♥ ● Jazz FM - USA - MP3 - 128 kbps                                 │
│   ● Classical Radio - UK - AAC - 192 kbps                           │
│   ● Rock Station - Germany - MP3 - 256 kbps                         │
└────────────────────────────────────────────────────────────────────┘
┌Player──────────────────────────────────────────────────────────────┐
│ ▶ Playing: Jazz FM                                                 │
│ Volume:  ████████████░░░░░░░░ 60%                                  │
│                                                                     │
│ Controls: Enter=Play Space=Pause/Resume S=Stop R=Reload            │
└────────────────────────────────────────────────────────────────────┘
┌Status──────────────────────────────────────────────────────────────┐
│ Keys: ↑/↓=Navigate []=Pages /=Search F=Favorite V=Vote Ctrl+C=Quit │
└────────────────────────────────────────────────────────────────────┘
```

## Installation

### Prerequisites

#### Linux
You need ALSA development libraries:

**Fedora/RHEL/CentOS:**
```bash
sudo dnf install alsa-lib-devel
```

**Debian/Ubuntu:**
```bash
sudo apt-get install libasound2-dev
```

**Arch Linux:**
```bash
sudo pacman -S alsa-lib
```

#### macOS
No additional dependencies required. Audio is handled through CoreAudio.

#### Windows
No additional dependencies required. Audio is handled through WASAPI.

### Building from Source

1. **Install Rust 1.83 or later** (if not already installed):
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. **Clone and build**:
   ```bash
   git clone https://github.com/osmo-systems/lazyradio.git
   cd lazyradio
   cargo build --release
   ```

3. **Run**:
   ```bash
   cargo run --release
   ```

Or install globally:
```bash
cargo install --path .
lazyradio
```

## Usage

### Quick Start

1. Launch LazyRadio
2. Press `/` to open the search popup
3. Type a query like `name=jazz country=usa` or just `jazz`
4. Use arrow keys to navigate results
5. Press `Enter` to play a station
6. Press `F` to add to favorites
7. Press `V` to vote and support the station on Radio Browser

### Advanced Search Guide

LazyRadio supports powerful field-based queries with autocomplete. Press `/` to open the search popup, then type your query using the syntax below.

#### Search Syntax

You can search using simple text (e.g., `jazz`) or field-based queries (e.g., `name=jazz country=usa`). Field-based searches allow precise filtering.

#### Available Search Fields

| Field | Description | Example |
|-------|-------------|---------|
| `name` | Station name | `name=BBC` |
| `country` | Country name | `country=Germany` |
| `countrycode` | ISO country code | `countrycode=US` |
| `state` | State/region | `state=California` |
| `language` | Language | `language=English` |
| `tag` | Genre/category tag | `tag=jazz` |
| `codec` | Audio codec | `codec=MP3` |
| `bitrate_min` | Minimum bitrate (kbps) | `bitrate_min=128` |
| `bitrate_max` | Maximum bitrate (kbps) | `bitrate_max=320` |
| `order` | Sort by (votes, clickcount, bitrate, etc.) | `order=votes` |
| `reverse` | Reverse sort order | `reverse=true` |
| `hidebroken` | Hide offline stations | `hidebroken=true` |
| `is_https` | Only HTTPS streams | `is_https=true` |
| `page` | Jump to specific page number | `page=5` |

#### Example Queries

```
# Simple text search (searches station name)
jazz

# Search by station name
name=BBC Radio

# High-quality jazz stations in the USA
tag=jazz country=USA bitrate_min=192

# Most popular stations
order=votes reverse=true hidebroken=true

# German classical music stations
tag=classical country=Germany language=German

# High-bitrate electronic music
tag=electronic bitrate_min=256 order=bitrate reverse=true
```

#### Autocomplete Features

- **Context-aware suggestions**: Autocomplete shows relevant fields and values as you type
- **Field icons**: Visual indicators help identify field types:
  - 🌍 Countries
  - 🗣️ Languages  
  - 🏷️ Tags/genres
  - 🎵 Codecs
  - 🔊 Bitrates
  - 📊 Sort orders
- **Arrow navigation**: Use ↑/↓ to select suggestions, Tab/Enter to accept
- **Smart completion**: Accepts with spaces, quotes values with spaces automatically

#### Default Query

When you first open the Browse tab, LazyRadio uses this default query:
```
order=votes reverse=true hidebroken=true
```

This shows the most popular, working stations first.

### Keyboard Shortcuts

#### Navigation
- `↑/↓`: Navigate through station lists one at a time
- `PgUp/PgDn`: Scroll up/down within current list by one page (visible area height)
- `Home/End`: Jump to first/last station in current list
- `Tab`: Next tab
- `Ctrl+Tab` or `Shift+Tab`: Previous tab

#### Pagination (API Requests)
- `[`: Load previous page from API
- `]`: Load next page from API
- Or use `page=N` in search query to jump to specific page

#### Search
- `/`: Open search popup
- `Esc`: Close search popup (or cancel current search)
- `Enter`: Submit search query
- `Tab`: Accept autocomplete suggestion
- `↑/↓`: Navigate autocomplete suggestions

#### Playback
- `Enter`: Play selected station
- `Space`: Pause/Resume playback
- `S`: Stop playback
- `R`: Reload current station (reconnect to stream)
- `+/=`: Volume up (+5%)
- `-/_`: Volume down (-5%)

#### Station Management
- `F`: Toggle favorite for selected station
- `V`: Vote for selected station on Radio Browser (helps improve the community database)

#### General
- `Ctrl+C`: Quit application
- `Esc`: Close popups / Clear error messages

### Data Storage

All user data is stored in platform-specific directories:
- **Linux**: `~/.local/share/lazyradio/`
- **macOS**: `~/Library/Application Support/lazyradio/`
- **Windows**: `%APPDATA%\lazyradio\`

Files stored:
- `favorites.toml`: Your favorite stations
- `history.toml`: Recently played stations (last 50)
- `search_history.toml`: Recent search queries (last 50)
- `session.toml`: Session state (last volume, last played station)
- `config.toml`: Application configuration
- `cache/`: Cached station lists
- `lazyradio.log`: Application logs

### Configuration

Edit `config.toml` to customize:

```toml
# Cache duration in seconds (default: 3600 = 1 hour)
cache_duration_secs = 3600

# Maximum number of history entries (default: 50)
max_history_entries = 50

# Maximum number of search history entries (default: 50)
max_search_history_entries = 50

# Default volume (0.0 to 1.0, default: 0.5)
default_volume = 0.5

# Maximum number of stations to fetch per query (default: 100)
station_limit = 100
```

## Architecture

### Project Structure

```
src/
├── main.rs           # Entry point and event loop
├── app.rs            # Application state management (3 tabs: Browse, Favorites, History)
├── config.rs         # Configuration management
├── api/              # Radio Browser API client
│   ├── client.rs     # HTTP client with automatic server discovery
│   └── models.rs     # Station data models
├── player/           # Audio playback engine
│   └── audio.rs      # Rodio-based streaming player
├── storage/          # Data persistence layer
│   ├── favorites.rs  # Favorites management
│   ├── history.rs    # Playback history tracking
│   ├── cache.rs      # API response caching
│   └── search_history.rs  # Search query history
├── search/           # Advanced search system
│   ├── parser.rs     # Query parser
│   ├── autocomplete.rs  # Autocomplete engine with 13 field types
│   └── mod.rs        # Search coordination
└── ui/               # Terminal UI components
    ├── layout.rs     # Main TUI layout with ratatui
    └── search_popup.rs  # Interactive search popup with autocomplete
```

### Key Technologies

- **[ratatui](https://github.com/ratatui-org/ratatui)**: Terminal UI framework for rich, interactive interfaces
- **[crossterm](https://github.com/crossterm-rs/crossterm)**: Cross-platform terminal manipulation
- **[rodio](https://github.com/RustAudio/rodio)**: Audio playback with codec support
- **[reqwest](https://github.com/seanmonstar/reqwest)**: Async HTTP client for API calls
- **[tokio](https://github.com/tokio-rs/tokio)**: Async runtime
- **[trust-dns-resolver](https://github.com/bluejekyll/trust-dns)**: DNS resolution for Radio Browser server discovery
- **[tracing](https://github.com/tokio-rs/tracing)**: Structured logging and diagnostics

## Radio Browser API

LazyRadio is built on the **[Radio Browser API](https://www.radio-browser.info/)**, a free, open-source, community-driven database of radio stations.

### About Radio Browser

Radio Browser provides:
- **40,000+ radio stations** from around the world
- **Community curation** - stations added and maintained by users
- **Free API access** - no registration, no rate limits
- **Multiple API servers** - distributed infrastructure for reliability
- **Rich metadata** - country, language, genre, codec, bitrate, and more
- **Voting system** - community votes help surface quality stations
- **Click tracking** - helps station owners understand their audience

### How LazyRadio Supports Radio Browser

- **Automatic server discovery**: LazyRadio discovers available API servers via DNS and load-balances requests
- **Vote integration**: Press `V` to vote for stations you love (increases their visibility)
- **Click tracking**: Every play is reported to help station analytics
- **Respectful API usage**: Smart caching reduces unnecessary API calls

### Contributing to Radio Browser

You can help improve the Radio Browser database:
- **Add stations**: Submit new stations at [radio-browser.info](https://www.radio-browser.info/)
- **Vote for quality**: Use LazyRadio's vote feature (`V` key) to highlight great stations
- **Report issues**: Flag broken or incorrect station data
- **Donate**: Support Radio Browser's infrastructure costs

## Troubleshooting

### Audio Issues

**No sound on Linux:**
- Ensure ALSA libraries are installed: `sudo dnf install alsa-lib-devel` (or equivalent for your distro)
- Check system volume and ensure audio output is not muted
- Verify audio works in other applications

**Crackling or stuttering:**
- Check your network connection stability
- Try a different station (some streams may have issues)
- Use the reload (`R`) command to reconnect

### Station Playback Issues

**"Failed to play station" error:**
- The station stream URL might be offline or changed
- Try another station
- Check logs in `~/.local/share/lazyradio/lazyradio.log`

**Station loads but no audio:**
- The codec might not be supported (though rodio supports most common formats: MP3, AAC, OGG, FLAC)
- Try pressing `R` to reload the station
- Check if other stations work

### API Issues

**"No Radio Browser servers found":**
- Check your internet connection
- DNS resolution might be blocked (requires access to `all.api.radio-browser.info`)
- Check firewall settings

**Slow loading:**
- Initial load fetches station lists which can take a few seconds
- Subsequent browsing uses cached data (configurable via `cache_duration_secs`)
- Try a more specific search query to reduce result size

### Search Issues

**Autocomplete not showing suggestions:**
- Keep typing - suggestions appear as you build your query
- Use field syntax (e.g., `name=`, `country=`) to trigger field-specific suggestions

**No results for query:**
- Check for typos in field names or values
- Try a broader search (e.g., remove filters)
- Use `hidebroken=false` to include offline stations in results

### Build Issues

**"alsa-sys" build fails:**
- Install ALSA development headers (see Installation Prerequisites)

**Other compilation errors:**
- Ensure you have Rust 1.83 or later: `rustup update`
- Try cleaning and rebuilding: `cargo clean && cargo build`

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guidelines.

### Quick Development Setup

Run in development mode:
```bash
cargo run
```

Run with debug logging:
```bash
RUST_LOG=debug cargo run
```

Run tests:
```bash
cargo test
```

## License

Licensed under either of:

- **MIT License** ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
- **Apache License 2.0** ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

## Acknowledgments

- **[Radio Browser](https://www.radio-browser.info/)** - For providing the free, community-driven radio station database that powers LazyRadio
- **[ratatui](https://github.com/ratatui-org/ratatui)** - For the excellent terminal UI framework
- **[rodio](https://github.com/RustAudio/rodio)** - For cross-platform audio playback
- **The Rust Community** - For all the amazing crates and tools that make projects like this possible

Built with ❤️ by [Mathieu Antoine](https://github.com/osmo-systems) and contributors.

## Changelog

See [CHANGELOG.md](CHANGELOG.md) for version history and release notes.
