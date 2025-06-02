#!/bin/bash
set -eux

# Install cross for cross-compilation
cargo install cross

# Start dbus
echo "Starting dbus..."
sudo dbus-daemon --system --fork

# Start avahi
echo "Starting avahi..."
sudo avahi-daemon --no-drop-root --daemonize --debug


