#!/bin/bash

# Test TUI startup and daemon connection
echo "=== Testing TUI Auto-start of Daemon ==="

# Check if daemon is running before
echo "Before: Checking for daemon processes..."
ps aux | grep player-daemon | grep -v grep || echo "  No daemon running (expected)"

# Start TUI with a timeout - send Ctrl+C after 2 seconds
echo ""
echo "Starting TUI (will stop after 2 seconds)..."
(sleep 2 && pkill -f "target/release/tui") &
./target/release/tui 2>&1 | head -20 || true

# Give it time to stabilize
sleep 1

# Check if daemon is still running after TUI closes
echo ""
echo "After: Checking for daemon processes..."
ps aux | grep player-daemon | grep -v grep || echo "  No daemon running"

# Check if socket exists
echo ""
echo "Checking for socket file..."
ls -la ~/Library/Application\ Support/lazyradio/.lazyradio-player.sock 2>/dev/null || echo "  Socket not found"

# Check daemon log
echo ""
echo "Last 5 lines of daemon log:"
tail -5 ~/Library/Application\ Support/lazyradio/lazyradio-daemon.log.2026-03-06 2>/dev/null || echo "  Log not found"
