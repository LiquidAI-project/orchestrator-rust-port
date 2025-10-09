#!/bin/bash
# Script assumes its being ran in the build folder

set -e

if [ -f .env ]; then
  export $(grep -v '^#' .env | sed 's/\s*#.*//' | xargs)
else
  echo "❌ Error: .env file not found. Please create one if you run into issues."
fi

# Start dbus
if ! pgrep -x dbus-daemon > /dev/null; then
  echo "🔌 Starting dbus-daemon..."
  rm -f /run/dbus/pid # Remove stale pid file if exists
  dbus-daemon --system --fork
else
  echo "✅ dbus-daemon already running."
fi

# Start avahi
if ! pgrep -x avahi-daemon > /dev/null; then
  echo "🌐 Starting avahi-daemon..."
  avahi-daemon --no-drop-root --daemonize --debug
else
  echo "✅ avahi-daemon already running."
fi

# Ensure necessary folder structure exists
mkdir -p instance/orchestrator/files
mkdir -p instance/orchestrator/init

# Start the orchestrator
./orchestrator



