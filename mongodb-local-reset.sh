#!/bin/bash
set -e

echo "🧹 Cleaning up local MongoDB data and config..."

DB_PATH="./mongo-data/db"
CONFIG_FILE="./mongod.conf"
LOCK_FILE="$DB_PATH/mongod.lock"

# Try deleting the lock file directly — fails if this throws an error, it indicates the mongodb is still running
if [ -f "$LOCK_FILE" ]; then
  rm "$LOCK_FILE"
fi

# Remove mongo data
echo "🗑  Removing MongoDB data and logs..."
rm -rf ./mongo-data

# Remove config
if [ -f "$CONFIG_FILE" ]; then
  echo "🗑  Removing mongod.conf..."
  rm "$CONFIG_FILE"
fi

echo "✅ Cleanup complete."
