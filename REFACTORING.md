# Workspace Refactoring Summary

## What Changed

This project has been refactored from a single-crate monolith into a Cargo workspace with multiple crates.

### Before
```
radm/
├── Cargo.toml (single package)
└── src/
    ├── lib.rs
    ├── main.rs
    ├── api/, config.rs, player/, search/, storage/
    └── bin/
        ├── tui/
        ├── cli/
        └── player-daemon/
```

### After
```
radm/ (workspace root)
├── Cargo.toml (workspace definition)
├── rad-core/     (library crate)
├── rad-tui/      (binary crate: rad)
├── rad-cli/      (binary crate: rad-cli)
└── rad-daemon/   (binary crate: _rad-daemon)
```

## Binary Name Changes

| Old Name       | New Name      | Purpose                    |
|----------------|---------------|----------------------------|
| `radt`         | `rad`         | Terminal UI (main app)     |
| `radc`         | `rad-cli`     | CLI control interface      |
| `rad-daemon`   | `_rad-daemon` | Background player daemon   |

The underscore prefix on `_rad-daemon` indicates it's an internal daemon not meant to be invoked directly by users.

## Key Benefits

1. **Faster Builds** - Cargo only rebuilds changed crates
2. **Clear Architecture** - Each crate has a well-defined purpose
3. **Better Testing** - Can test each component independently
4. **Flexible Deployment** - Ship only the binaries you need
5. **Easier Maintenance** - Changes to core logic don't require rebuilding UI code

## Migration Notes

### For Users
- Install the TUI with: `cargo install --path rad-tui`
- Binary name changed from `radt` to `rad`
- CLI binary changed from `radc` to `rad-cli`
- All data directories remain the same (no migration needed)

### For Developers
- Import core functionality with `use rad_core::*` instead of `use radm::*`
- Build workspace with: `cargo build --workspace`
- Build specific crate: `cargo build -p rad-tui`
- See [WORKSPACE.md](WORKSPACE.md) for detailed development guide

## Files Changed

### Created
- `Cargo.toml` (workspace root)
- `rad-core/Cargo.toml` and source files
- `rad-tui/Cargo.toml` and source files
- `rad-cli/Cargo.toml` and source files
- `rad-daemon/Cargo.toml` and source files
- `WORKSPACE.md` (documentation)
- `REFACTORING.md` (this file)

### Modified
- `README.md` - Updated build instructions and binary names
- All source files - Changed imports from `radm::` to `rad_core::`

### Preserved
- All existing functionality remains unchanged
- Data storage locations unchanged
- API compatibility maintained
- User experience identical (except binary names)

## Testing

All crates build successfully:
```bash
cargo build --workspace --release
```

Binaries verified:
- ✅ `target/release/rad` (3.0M) - TUI
- ✅ `target/release/rad-cli` (2.7M) - CLI
- ✅ `target/release/_rad-daemon` (5.3M) - Daemon
- ✅ `target/release/librad_core.rlib` (6.2M) - Core library

## Next Steps

1. Update CI/CD pipelines to build workspace
2. Update installation scripts to reference new binary names
3. Consider publishing `rad-core` as a separate crate on crates.io
4. Remove old `src/` directory backup after verification
5. Update any external documentation referencing old binary names
