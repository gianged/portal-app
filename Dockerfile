# syntax=docker/dockerfile:1.6
#
# Multi-target build for the Rust workspace.
#
# Targets:
#   runtime  — minimal slim image with a single compiled binary. Pick which
#              binary via the BINARY_NAME build arg (default: server).
#   dev      — full toolchain + cargo-watch, source bind-mounted at runtime.
#              Command supplied by docker-compose.dev.yml.
#
# Build examples:
#   docker build --target runtime --build-arg BINARY_NAME=server  -t portal/server  .
#   docker build --target runtime --build-arg BINARY_NAME=workers -t portal/workers .
#   docker build --target dev -t portal/rust-dev .

# ---- chef: shared base with cargo-chef for dependency caching --------------
FROM rust:1.94-bookworm AS chef
RUN cargo install cargo-chef --locked --version "^0.1"
WORKDIR /app

# ---- planner: derive the dependency recipe -------------------------------
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path /recipe.json

# ---- builder: cook deps (cached), then build the chosen binary -----------
FROM chef AS builder
ARG BINARY_NAME=server
COPY --from=planner /recipe.json /recipe.json
RUN cargo chef cook --release --recipe-path /recipe.json
COPY . .
RUN cargo build --release --bin "${BINARY_NAME}"

# ---- runtime: slim debian + ca-certs + the single binary -----------------
FROM debian:bookworm-slim AS runtime
ARG BINARY_NAME=server
# curl backs the server service's compose healthcheck (probes /healthz).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/${BINARY_NAME} /usr/local/bin/app
EXPOSE 8080
ENV RUST_BACKTRACE=1
CMD ["/usr/local/bin/app"]

# ---- dev: rust toolchain + cargo-watch, source bind-mounted at runtime ---
FROM rust:1.94-bookworm AS dev
RUN cargo install cargo-watch --locked --version "^8"
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates pkg-config \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
# CARGO_TARGET_DIR sits OUTSIDE /app so the named volume doesn't overlay the
# bind-mounted source — known WSL2 footgun.
ENV CARGO_TARGET_DIR=/build/target
ENV RUST_BACKTRACE=1
EXPOSE 8080
# command supplied by docker-compose.dev.yml
