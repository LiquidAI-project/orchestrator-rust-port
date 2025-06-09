#!/bin/bash
# Script assumes its being ran in the build folder

set -e

if [ -f .env ]; then
  export $(grep -v '^#' .env | sed 's/\s*#.*//' | xargs)
else
  echo "❌ Error: .env file not found. Please create one before running this script."
  exit 1
fi

# Set defaults if not in .env
export REACT_APP_API_URL="${REACT_APP_API_URL:-http://localhost:3000}"
export PORT="${PORT:-3000}"

# Start dbus
if ! pgrep -x dbus-daemon > /dev/null; then
  echo "🔌 Starting dbus-daemon..."
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

# Start the orchestrator
./orchestrator



