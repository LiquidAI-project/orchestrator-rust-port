# Build and compile the orchestrator
FROM rust AS build_rust
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN apt update -y \
    && apt upgrade -y \
    && apt install libclang-dev xorg-dev libxcb-shape0-dev libxcb-xfixes0-dev clang avahi-daemon libavahi-client-dev -y
RUN cargo build --release

# Build the react frontend for the orchestrator
FROM node AS build_node
ARG REACT_APP_API_URL
ARG PORT
ENV REACT_APP_API_URL=$REACT_APP_API_URL
ENV PORT=$PORT
COPY wasmiot-orchestrator-webgui wasmiot-orchestrator-webgui
WORKDIR /wasmiot-orchestrator-webgui/frontend
RUN npm install && npm run build
    
# Copy compiled orchestrator to final runtime image.
FROM debian:bullseye-slim AS runtime
LABEL org.opencontainers.image.source="https://github.com/LiquidAI-project/orchestrator-rust-port"
WORKDIR /app
COPY --from=build_rust /app/target/release/orchestrator /app/
COPY --from=build_node /wasmiot-orchestrator-webgui/frontend/build /app/wasmiot-orchestrator-webgui/frontend/build
CMD ["/app/orchestrator"]