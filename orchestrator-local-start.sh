#!/bin/bash
set -e

# === Config ===
FRONTEND_BUILD_DIR="build/frontend"
RELEASE_MODE=false
FORCE_FRONTEND_BUILD=false
NO_RUN=false

# === Functions ===

usage() {
  echo "Usage: $0 [--release] [--force-frontend-build] [--help]"
  echo
  echo "Options:"
  echo "  --release                 Build Rust backend in release mode"
  echo "  --force-frontend-build    Force rebuild of frontend, even if already built"
  echo "  --no-run                  Only build, do not run the orchestrator"
  echo "  --help                    Show this help message"
  exit 0
}

# === Parse arguments ===

while [[ $# -gt 0 ]]; do
  case "$1" in
    --release)
      RELEASE_MODE=true
      shift
      ;;
    --force-frontend-build)
      FORCE_FRONTEND_BUILD=true
      shift
      ;;
    --no-run)
      NO_RUN=true
      shift
      ;;
    --help)
      usage
      ;;
    *)
      echo "❌ Unknown option: $1"
      usage
      ;;
  esac
done

# === Load .env ===

if [ -f .env ]; then
  export $(grep -v '^#' .env | sed 's/\s*#.*//' | xargs)
else
  echo "❌ Error: .env file not found. Please create one before running this script."
  exit 1
fi

# Set defaults if not in .env
export REACT_APP_API_URL="${REACT_APP_API_URL:-http://localhost:3000}"
export PORT="${PORT:-3000}"

# === Step 1: Build frontend ===

if [[ "$FORCE_FRONTEND_BUILD" == true || ! -d "$FRONTEND_BUILD_DIR" || -z "$(ls -A $FRONTEND_BUILD_DIR 2>/dev/null)" ]]; then
  echo "⚙️  Building React frontend..."
  cd wasmiot-orchestrator-webgui/frontend
  npm install
  npm run build
  cd ../..

  echo "📁 Copying frontend build to $FRONTEND_BUILD_DIR..."
  rm -rf build
  mkdir -p "$FRONTEND_BUILD_DIR"
  cp -r wasmiot-orchestrator-webgui/frontend/build/* "$FRONTEND_BUILD_DIR/"
else
  echo "✅ Frontend already built. Skipping. Use --force-frontend-build to rebuild."
fi

# === Step 2: Build Rust backend ===

echo "🦀 Building Rust backend..."
if [[ "$RELEASE_MODE" == true ]]; then
  cargo build --release
  BIN_PATH="./target/release/orchestrator"
else
  cargo build
  BIN_PATH="./target/debug/orchestrator"
fi

# === Step 3: Run backend ===

if [[ "$NO_RUN" == true ]]; then
  echo "✅ Build completed. Skipping run (--no-run)."
else
  echo "🚀 Starting orchestrator on port $PORT..."
  exec "$BIN_PATH"
fi
