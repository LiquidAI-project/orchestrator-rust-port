# Build and compile the orchestrator and react frontend
FROM rust AS build_phase
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN apt update -y \
    && apt upgrade -y \
    && apt install -y \
    libclang-dev \
    xorg-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    clang \
    avahi-daemon \
    libavahi-client-dev \
    nodejs \
    npm
COPY wasmiot-orchestrator-webgui wasmiot-orchestrator-webgui
COPY entrypoint.sh entrypoint.sh
COPY orchestrator-local-start.sh orchestrator-local-start.sh
RUN ./orchestrator-local-start.sh --no-run
    
# Copy compiled orchestrator to final runtime image.
FROM debian:bookworm-slim AS runtime
RUN apt update -y \
    && apt upgrade -y \
    && apt install -y \
    libclang-dev \
    xorg-dev \
    libxcb-shape0-dev \
    libxcb-xfixes0-dev \
    libv4l-dev \
    v4l-utils \
    clang \
    avahi-daemon \
    libavahi-client-dev \
    avahi-utils \
    dbus \
    procps
LABEL org.opencontainers.image.source="https://github.com/LiquidAI-project/orchestrator-rust-port"
RUN mkdir -p /run/dbus
RUN rm -rf /run/dbus/*
WORKDIR /app
COPY --from=build_phase /app/build /app/build
WORKDIR build
CMD ["./entrypoint.sh"]