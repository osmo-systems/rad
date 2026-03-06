# LazyRadio Architecture

## Overview

LazyRadio has been refactored with a **headless player daemon** architecture. The audio player runs as a separate background process that can continue playing music even after the user interface closes. This separation provides:

- **Reduced memory footprint**: 7-10 MB daemon vs 50-90 MB TUI application
- **Persistent playback**: Music continues after closing UI
- **Multi-client support**: Multiple UIs can connect to the same daemon
- **Auto-lifecycle management**: Daemon auto-starts when needed, auto-shuts down after 30 minutes of inactivity

## Architecture Diagram

```
┌─────────────────────────────────────┐
│         TUI/CLI Applications        │
│  (Temporary, can be closed)         │
└──────────────┬──────────────────────┘
               │ Unix Socket IPC
               │ JSON Messages
               ▼
┌─────────────────────────────────────┐
│    Player Daemon (Persistent)       │
│   Auto-starts/Stops automatically   │
│  ├─ Audio Player (rodio)            │
│  └─ Unix Socket Listener            │
└─────────────────────────────────────┘
```

## Project Structure

```
src/
├── lib.rs              # Library entry point
├── main.rs             # Stub - instructions for using binaries
├── ipc.rs              # IPC protocol types (JSON messages)
├── ipc_client.rs       # Client for connecting to daemon
├── api/                # Radio Browser API client
├── config.rs           # Configuration management
├── player/             # Audio playback engine
├── search/             # Query parsing & autocomplete
├── storage/            # Persistence layer
└── bin/
    ├── tui/            # Terminal UI binary (connects to daemon)
    │   ├── main.rs     # TUI entry point
    │   ├── app.rs      # TUI app state
    │   └── ui/         # Ratatui UI components
    ├── cli/            # Command-line binary (connects to daemon)
    │   └── main.rs     # CLI entry point with command dispatcher
    └── player-daemon/  # Player daemon binary (persistent background process)
        └── main.rs     # Daemon entry point - socket listener & player
```

## IPC Protocol

### Communication Pattern
- **Transport**: Unix domain socket at `~/.config/lazyradio/.lazyradio-player.sock`
- **Format**: JSON Lines (one JSON object per line)
- **Pattern**: Request/Response (client sends message, daemon responds)

### Message Types

**ClientMessage** (Client → Daemon):
```rust
enum ClientMessage {
    Play { name: String, url: String },
    Pause,
    Resume,
    Stop,
    SetVolume(f32),           // 0.0 to 1.0
    Reload,                   // Reload current stream
    ClearError,               // Clear error state
    GetStatus,                // Request current state
    Shutdown,                 // Gracefully shut down daemon
}
```

**DaemonMessage** (Daemon → Client):
```rust
enum DaemonMessage {
    Status(PlayerStateDto),   // Response to GetStatus
    Ok,                       // Successful command
    Error(String),            // Command error
    Shutdown,                 // Daemon shutting down
}
```

## Module Organization

### Library Modules (src/)

- **`ipc.rs`** - IPC protocol types and conversions
  - `ClientMessage` - Commands from client to daemon
  - `DaemonMessage` - Responses from daemon to client
  - `PlayerStateDto` - Serializable player state

- **`ipc_client.rs`** - Client connection management
  - `PlayerDaemonClient` - Auto-starts daemon, handles binary discovery
  - `PlayerDaemonConnection` - Active connection for sending commands

- **`api`** - Radio Browser API interaction
  - `RadioBrowserClient` - Main API client
  - `Station` - Radio station data structure

- **`config`** - Configuration & data directory management
  - `Config` - Application configuration with station persistence

- **`player`** - Audio playback abstraction
  - `AudioPlayer` - Playback engine (used only by daemon)
  - `PlayerState` - Playback state

- **`search`** - Query parsing and autocomplete
- **`storage`** - Data persistence

### Binary Modules (src/bin/)

#### Player Daemon (`player-daemon`)
Background process that handles audio playback:

- **`main.rs`** - Unix socket listener and player control
  - Handles multiple client connections
  - Single-threaded tokio runtime (for non-Send AudioPlayer)
  - 30-minute idle timeout with monitoring
  - JSON-based IPC message handling

**Key Features:**
- Auto-starts when first client connects
- Continues playing after clients disconnect
- Auto-shuts down after 30 minutes with no clients and not playing
- Logs to `~/.config/lazyradio/lazyradio-daemon.log.YYYY-MM-DD`

#### TUI Binary (`tui`)
Terminal UI using Ratatui (connects to daemon):

- **`main.rs`** - Initialization, daemon connection, event loop
- **`app.rs`** - Application state, async player control via daemon
- **`ui/`** - Ratatui UI components

**Run:** `cargo run --bin tui`

#### CLI Binary (`cli`)
Command-line tool for programmatic control:

- **`main.rs`** - Command dispatcher
- **Commands**: `status`, `play`, `pause`, `resume`, `stop`, `volume`, `play-url`, `search`, `quit`, `help`

**Run:** `cargo run --bin cli`

**Example Usage:**
```bash
# Get player status
radiocli status

# Search for stations
radiocli search "jazz"

# Play a station (saves as last played)
radiocli play-url "Station Name" "http://stream.url"

# Control playback
radiocli pause
radiocli resume
radiocli stop

# Control volume
radiocli volume 75           # Set to 75%
radiocli volume --up 10      # Increase by 10%
radiocli volume --down 5     # Decrease by 5%
```

## Daemon Lifecycle

### Auto-Start
When TUI/CLI tries to connect:
1. Check if daemon is running via Unix socket
2. If not running, spawn daemon process using binary discovery
3. Wait for socket to become available
4. Connect and proceed

### Auto-Shutdown
Daemon monitors activity every minute:
1. If idle (no clients + not playing) for 30 minutes → shutdown
2. If client connects or music starts → reset idle timer
3. Graceful shutdown on `Shutdown` command

### Binary Discovery
- Both binaries built in same directory: `target/release/`
- Client finds daemon binary via `std::env::current_exe()`
- Can spawn from TUI, CLI, or manual invocation

## Data Flow

### Playing a Station via CLI

```
User: radiocli play-url "Station" "http://..."
  ↓
CLI: Connect to daemon (auto-start if needed)
  ↓
CLI: Send ClientMessage::Play(name, url)
  ↓
Daemon: Receive and parse JSON message
  ↓
Daemon: Pass to AudioPlayer via command channel
  ↓
Daemon: Send DaemonMessage::Ok back to client
  ↓
CLI: Display status and exit
  ↓
Daemon: Continue playing after CLI exits ✓
```

## Building & Running

### Build Everything
```bash
cargo build --release
```

Outputs:
- `target/release/tui` - Terminal UI
- `target/release/cli` - Command-line tool
- `target/release/player-daemon` - Background daemon

### Run TUI
```bash
./target/release/tui
```

### Run CLI
```bash
./target/release/cli status
./target/release/cli search "jazz"
./target/release/cli play-url "Station" "http://url"
```

### Run Daemon Manually (usually auto-starts)
```bash
./target/release/player-daemon
```

## Configuration & Data

### Directories
- **macOS**: `~/Library/Application Support/lazyradio/`
- **Linux**: `~/.config/lazyradio/`
- **Windows**: `%APPDATA%\lazyradio\`

### Files
- `config.toml` - Application configuration (volume, last station, etc.)
- `favorites.toml` - Favorite stations
- `history.toml` - Play history
- `.lazyradio-player.sock` - Unix domain socket (created at runtime)
- `lazyradio-daemon.log.*` - Daemon logs (dated)
- `lazyradio-tui.log.*` - TUI logs (dated)
- `lazyradio-cli.log.*` - CLI logs (dated)

## Why This Architecture?

1. **Persistent Playback** - Music continues after UI closes
2. **Low Memory** - Daemon is lean, UI can be closed to save memory
3. **Multi-Client** - Multiple UIs can connect to same daemon
4. **Auto-Lifecycle** - No manual daemon management needed
5. **Reusable** - Library and IPC can be used by other applications
6. **Extensible** - Easy to add new UI clients (web, mobile, etc.)

## Adding New UI Clients

To add a new client (e.g., web UI):

1. Create `src/bin/myui/main.rs`
2. Add to `Cargo.toml`:
   ```toml
   [[bin]]
   name = "myui"
   path = "src/bin/myui/main.rs"
   ```
3. Use the library:
   ```rust
   use lazyradio::{PlayerDaemonClient, PlayerDaemonConnection};
   
   let client = PlayerDaemonClient::new()?;
   let mut conn = client.connect().await?;
   let status = conn.get_status().await?;
   ```

## Testing

### Manual Testing Workflow
1. Build: `cargo build --release`
2. Start fresh: `pkill -f player-daemon; rm ~/.config/lazyradio/.lazyradio-player.sock`
3. Test CLI: `./target/release/cli status` → daemon auto-starts
4. Test playing: `./target/release/cli play-url "Test" "http://url"`
5. Verify persistence: Kill CLI, daemon continues playing
6. Reconnect: `./target/release/cli status` → reconnects to running daemon
