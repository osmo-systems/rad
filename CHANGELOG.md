# Changelog

All notable changes to LazyRadio will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Planned
- Playlist support (queue multiple stations)
- Custom station URLs
- Themes and color schemes
- Mouse support
- Station metadata display (now playing information)
- Export/import favorites

## [0.1.0] - 2026-03-01

### Added - Initial Release

LazyRadio v0.1.0 is the first public release of this terminal-based web radio player, built from the ground up with Rust and powered by the Radio Browser API.

#### Core Features
- **Three-tab interface**: Browse, Favorites, and History tabs for easy navigation
- **Advanced search with autocomplete**: Field-based query syntax supporting 13 search fields
  - Fields: name, country, countrycode, state, language, tag, codec, bitrate_min, bitrate_max, order, reverse, hidebroken, is_https
  - Context-aware autocomplete with visual field icons
  - Search history stored locally (last 50 queries)
- **Radio Browser integration**: Access to 40,000+ community-curated radio stations
  - Automatic API server discovery via DNS
  - Load balancing across multiple servers
  - Station voting to support the community database
  - Click tracking for station analytics
- **Full playback controls**: Play, pause, stop, reload, and volume adjustment
- **Favorites management**: Save and organize favorite stations locally
- **Playback history**: Automatic tracking of recently played stations (last 50)
- **Session persistence**: Restores last volume and played station on startup
- **Smart caching**: Reduces API load with configurable cache duration
- **Cross-platform support**: Works on Linux, macOS, and Windows
- **Keyboard-driven interface**: Complete control without touching the mouse

#### User Interface
- Clean, responsive TUI built with ratatui
- Station list with rich metadata display (name, country, codec, bitrate)
- Player panel with playback status and volume indicator
- Interactive search popup with real-time autocomplete
- Pagination support for large result sets (N/P keys)
- Status bar with helpful keyboard shortcuts
- Error popups with clear messaging

#### Technical Stack
- **Rust 1.83+**: Modern, safe systems programming
- **ratatui**: Terminal UI framework for rich interfaces
- **crossterm**: Cross-platform terminal manipulation
- **rodio**: Audio playback with multi-codec support (MP3, AAC, OGG, FLAC)
- **tokio**: Async runtime for network operations
- **reqwest**: HTTP client for API communication
- **trust-dns-resolver**: DNS resolution for server discovery
- **stream-download**: Efficient HTTP audio streaming
- **tracing**: Structured logging for debugging

#### Data Storage
- Platform-specific data directories (XDG on Linux, standard locations on macOS/Windows)
- TOML format for favorites, history, and configuration
- File-based caching for API responses
- Comprehensive logging to disk

#### Documentation
- Comprehensive README with installation, usage, and troubleshooting
- Contributing guidelines for developers
- Dual licensing (MIT OR Apache-2.0)
- GitHub issue and PR templates

#### Known Limitations
- No playlist support (single station playback only)
- No custom station URLs (Radio Browser API only)
- No theme customization (default colors)
- No mouse support (keyboard only)
- No "now playing" metadata (station name/URL only)
- Search limited to Radio Browser's supported fields
- Audio visualization not included in this release

### Changed
- N/A (initial release)

### Deprecated
- N/A (initial release)

### Removed
- N/A (initial release)

### Fixed
- N/A (initial release)

### Security
- No external telemetry or data collection
- All user data stored locally
- HTTPS support for secure station streams

## Release Process

LazyRadio follows semantic versioning:
- **Major version (X.0.0)**: Breaking changes, major rewrites
- **Minor version (0.X.0)**: New features, non-breaking changes
- **Patch version (0.0.X)**: Bug fixes, minor improvements

Releases are published on GitHub with pre-built binaries for Linux, macOS, and Windows.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for information on how to contribute to LazyRadio.

## Links

- [Repository](https://github.com/osmo-systems/lazyradio)
- [Issue Tracker](https://github.com/osmo-systems/lazyradio/issues)
- [Radio Browser API](https://www.radio-browser.info/)

---

**Legend:**
- `Added` for new features
- `Changed` for changes in existing functionality
- `Deprecated` for soon-to-be removed features
- `Removed` for now removed features
- `Fixed` for any bug fixes
- `Security` in case of vulnerabilities
