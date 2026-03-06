# LazyRadio Architecture

## Overview

LazyRadio has been refactored to separate the core library logic from the user interfaces (binaries). This allows the radio player functionality to be consumed programmatically while also providing multiple CLI interfaces.

## Project Structure

```
src/
├── lib.rs              # Library entry point - exports core modules
├── main.rs             # Stub - instructions for using binaries
├── api/                # Radio Browser API client (Library)
├── config.rs           # Configuration management (Library)
├── player/             # Audio playback engine (Library)
├── search/             # Query parsing & autocomplete (Library)
├── storage/            # Persistence layer (Library)
└── bin/
    ├── tui/            # Terminal UI binary
    │   ├── main.rs     # TUI entry point
    │   ├── app.rs      # TUI app state (UI-specific)
    │   └── ui/         # Ratatui UI components
    │       ├── layout.rs
    │       └── search_popup.rs
    └── cli/            # Command-line binary
        └── main.rs     # CLI entry point with command dispatcher
```

## Module Organization

### Library Modules (src/)
These modules form the core library and are reusable:

- **`api`** - Radio Browser API interaction
  - `RadioBrowserClient` - Main API client
  - `Station` - Radio station data structure
  - Models and DTOs

- **`config`** - Configuration & data directory management
  - `Config` - Application configuration
  - Functions for loading/saving preferences

- **`player`** - Audio playback abstraction
  - `AudioPlayer` - Playback engine
  - `PlayerCommand` - Command enum for player control
  - `PlayerState` - Playback state

- **`search`** - Query parsing and autocomplete
  - `SearchQuery` - Parsed search parameters
  - `parse_query()` - Query parser
  - `AutocompleteData` - Autocomplete suggestions

- **`storage`** - Data persistence
  - `FavoritesManager` - Manage favorite stations
  - `HistoryManager` - Play history
  - `CacheManager` - API response caching
  - `SearchHistoryManager` - Recent search queries

### Binary Modules (src/bin/)

#### TUI Binary (`tui`)
The original terminal UI using Ratatui:

- **`app.rs`** - Application state (UI-specific, depends on `SearchPopup`)
- **`ui/`** - Ratatui UI components
  - `layout.rs` - Main UI drawing
  - `search_popup.rs` - Search input popup widget

**Run:** `cargo run --bin tui`

#### CLI Binary (`cli`)
A command-line interface for programmatic control:

- **`main.rs`** - REPL-style command dispatcher
- Commands: help, search, play, pause, stop, volume, status, exit

**Run:** `cargo run --bin cli`

## Why This Architecture?

1. **Reusability** - The library can be used in other applications
2. **Separation of Concerns** - UI logic is separated from business logic
3. **Multiple Interfaces** - Support different UIs without code duplication
4. **Testability** - Library modules can be tested independently
5. **Extensibility** - Easy to add new binaries (web UI, API server, etc.)

## Using the Library

To use LazyRadio as a library in another project:

```rust
use lazyradio::{RadioBrowserClient, AudioPlayer, PlayerCommand};

#[tokio::main]
async fn main() {
    let api_client = RadioBrowserClient::new().await.unwrap();
    let (player, mut rx) = AudioPlayer::new().unwrap();
    let tx = player.get_command_sender();
    
    // Play a station
    tx.send(PlayerCommand::Play(
        "Station Name".to_string(),
        "http://stream.url".to_string()
    )).unwrap();
}
```

## Building & Running

### Build Everything
```bash
cargo build --release
```

### Run TUI
```bash
cargo run --bin tui
```

### Run CLI
```bash
cargo run --bin cli
```

### Build Library Only
```bash
cargo build --lib
```

## Adding New Binaries

To add a new binary (e.g., a web API):

1. Create `src/bin/myapp/main.rs`
2. Add to `Cargo.toml`:
   ```toml
   [[bin]]
   name = "myapp"
   path = "src/bin/myapp/main.rs"
   ```
3. Import from the library: `use lazyradio::*;`
4. Run: `cargo run --bin myapp`

## Dependencies

- **Library**: core dependencies (tokio, serde, reqwest, rodio, etc.)
- **TUI Binary**: library + ratatui, crossterm
- **CLI Binary**: library only

UI libraries are only required by specific binaries, not the core library.
