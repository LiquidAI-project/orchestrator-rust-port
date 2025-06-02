#!/bin/bash
set -e

echo "🔍 Stopping local MongoDB..."

# Default db path used by start script
DB_PATH="./mongo-data/db"

# Check if mongod is installed
if ! command -v mongod &> /dev/null; then
  echo "❌ Error: MongoDB (mongod) is not installed or not in PATH."
  exit 1
fi

# Ensure the DB path exists
if [ ! -d "$DB_PATH" ]; then
  echo "⚠️  MongoDB data directory not found at '$DB_PATH'."
  echo "Nothing to stop."
  exit 0
fi

# Attempt clean shutdown
echo "⏹  Shutting down MongoDB using --shutdown..."
mongod --dbpath "$DB_PATH" --shutdown

echo "✅ MongoDB stopped successfully."
