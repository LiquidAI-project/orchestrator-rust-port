# Build and compile the orchestrator
FROM rust:1.86-bullseye AS build_stage
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN apt update -y && apt upgrade -y
RUN cargo build --release
    
# Copy compiled orchestrator to final runtime image.
FROM debian:bullseye-slim AS runtime
RUN apt update -y && apt upgrade -y
LABEL org.opencontainers.image.source="https://github.com/LiquidAI-project/orchestrator-rust-port"
WORKDIR /app
COPY --from=build_stage /app/target/release/orchestrator /app/
CMD ["/app/orchestrator"]