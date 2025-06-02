#!/bin/bash
set -e

echo "🔍 Checking for MongoDB (mongod)..."

# Check for mongod
if ! command -v mongod &> /dev/null; then
  echo "❌ Error: MongoDB (mongod) is not installed. Please install it before trying to run this script."
  exit 1
fi

echo "✅ MongoDB is installed."

# Check for .env
if [ -f .env ]; then
  export $(grep -v '^#' .env | sed 's/\s*#.*//' | xargs)
else
  echo "❌ Error: .env file not found. Copy the .env.example file, rename it to .env, and modify its contents to suit your needs."
  exit 1
fi

# Set defaults
MONGO_PORT=${MONGO_PORT:-27017}
MONGO_ROOT_USERNAME=${MONGO_ROOT_USERNAME:-root}
MONGO_ROOT_PASSWORD=${MONGO_ROOT_PASSWORD:-example}

# Ensure data folders exist
mkdir -p ./mongo-data/db
mkdir -p ./mongo-data/config

# Create minimal config file if missing
CONFIG_FILE="./mongod.conf"

if [ ! -f "$CONFIG_FILE" ]; then
  echo "Creating default mongod.conf..."
  echo "systemLog:
  destination: file
  path: mongo-data/mongod.log
  logAppend: true
storage:
  dbPath: mongo-data/db
net:
  port: $MONGO_PORT
  bindIp: 127.0.0.1" > "$CONFIG_FILE"
fi

# Start mongod
echo "➡️  Starting mongod on port $MONGO_PORT..."
mongod --config "$CONFIG_FILE" --fork

echo "✅ MongoDB is now running at http://localhost:$MONGO_PORT"
