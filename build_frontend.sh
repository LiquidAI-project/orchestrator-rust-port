#!/bin/bash

set -e

# Navigate to the webgui folder and install dependencies
cd wasmiot-orchestrator-webgui/frontend
echo "Installing frontend dependencies..."
npm install

# Build the frontend from scratch
echo "Building the frontend..."
npm run build

# Reset submodule changes that occur from build process
echo "Resetting submodule changes..."
cd ..
git checkout -- .
git clean -fd

# Go back and clean the old frontend assets
echo "Removing old frontend..."
cd ..
rm -rf static/frontend

# Copy newly built files to static folder
echo "Copying built frontend files..."
mkdir -p static/frontend
cp -r wasmiot-orchestrator-webgui/frontend/build/* static/frontend/

echo "Frontend build completed successfully."
