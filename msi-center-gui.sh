#!/bin/bash
# MSI Center Linux GUI Launcher
# Requires root for hardware control

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUI_BIN="$SCRIPT_DIR/target/release/msi-center-gui"

if [ ! -f "$GUI_BIN" ]; then
    echo "Error: GUI binary not found. Run 'cargo build --release' first."
    exit 1
fi

if [ "$EUID" -ne 0 ]; then
    echo "MSI Center Linux requires root access for hardware control."
    echo "Launching with pkexec..."
    pkexec "$GUI_BIN" "$@"
else
    "$GUI_BIN" "$@"
fi
