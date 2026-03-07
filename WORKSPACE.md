# Workspace Structure

This project is organized as a Cargo workspace with multiple crates for better modularity, faster builds, and clearer separation of concerns.

## Workspace Members

### `rad-core` (Library Crate)
The core library containing all shared functionality:
- **API Client** (`api/`) - Radio Browser API integration
- **Configuration** (`config.rs`) - Application config and settings
- **IPC** (`ipc.rs`, `ipc_client.rs`) - Inter-process communication between binaries
- **Audio Player** (`player/`) - Audio playback engine using rodio
- **Search** (`search/`) - Advanced search query parsing and autocomplete
- **Storage** (`storage/`) - Local data persistence (favorites, history, cache)

All other crates depend on `rad-core` for their shared functionality.

### `rad-tui` (Binary Crate)
Terminal User Interface application.
- **Binary name**: `rad`
- **Dependencies**: rad-core, ratatui, crossterm, tokio
- **Features**: 
  - Interactive TUI with keyboard navigation
  - Real-time player status display
  - Advanced search with autocomplete
  - Favorites and history management
  - Multi-tab interface (Browse, Favorites, History)

### `rad-cli` (Binary Crate)
Command-line interface for controlling the player.
- **Binary name**: `rad-cli`
- **Dependencies**: rad-core, cliclack, tokio
- **Features**:
  - One-liner commands (pause, resume, volume, etc.)
  - Interactive station search
  - Direct search with filters
  - Player status queries

### `rad-daemon` (Binary Crate)
Headless player daemon that runs in the background.
- **Binary name**: `_rad-daemon`
- **Dependencies**: rad-core, tokio
- **Features**:
  - Persistent audio playback
  - Unix socket IPC server
  - Auto-shutdown after idle timeout
  - State persistence across client connections

## Building

### Build entire workspace:
```bash
cargo build --workspace
```

### Build specific crate:
```bash
cargo build -p rad-tui      # TUI only
cargo build -p rad-cli      # CLI only
cargo build -p rad-daemon   # Daemon only
cargo build -p rad-core     # Core library only
```

### Build in release mode:
```bash
cargo build --workspace --release
```

## Dependency Management

This workspace uses a **hybrid dependency management approach**:

- **Workspace-level dependencies** (defined in root `Cargo.toml`):
  - Common dependencies shared across multiple crates: tokio, anyhow, serde, tracing, etc.
  - Ensures version consistency across the workspace
  
- **Crate-specific dependencies** (defined in each crate's `Cargo.toml`):
  - Dependencies unique to a specific binary (e.g., ratatui for TUI, cliclack for CLI)
  - Core library dependencies (reqwest, rodio, etc.)

## Benefits of Workspace Structure

1. **Faster Incremental Builds** - Only rebuild changed crates
2. **Clear Separation of Concerns** - Each crate has a well-defined purpose
3. **Better Code Organization** - Easier to navigate and understand
4. **Independent Testing** - Test each crate in isolation
5. **Flexible Deployment** - Build and ship only the binaries you need
6. **Easier Maintenance** - Changes to core logic don't require rebuilding UI code

## Development Workflow

### Running from workspace root:
```bash
# Run TUI
cargo run -p rad-tui

# Run CLI
cargo run -p rad-cli -- help

# Run daemon
cargo run -p rad-daemon
```

### Testing:
```bash
# Test entire workspace
cargo test --workspace

# Test specific crate
cargo test -p rad-core
```

### Linting:
```bash
# Check entire workspace
cargo clippy --workspace

# Check specific crate
cargo clippy -p rad-tui
```
