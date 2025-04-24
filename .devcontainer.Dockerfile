FROM mcr.microsoft.com/vscode/devcontainers/rust:latest

# Install some dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    cmake \
    build-essential 
