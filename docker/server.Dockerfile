# syntax=docker/dockerfile:1.4
# Multi-stage Docker build for Breakpoint server + WASM client
#
# Stage 1: Build server binary and WASM client
# Stage 2: Slim runtime image with just the binary + web assets

# ── Builder ──────────────────────────────────────────────────────
FROM rust:1.93-slim AS builder

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install wasm-pack for WASM client build
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install wasm-pack --locked

WORKDIR /build

# Copy workspace manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./
COPY crates/breakpoint-core/Cargo.toml crates/breakpoint-core/Cargo.toml
COPY crates/breakpoint-server/Cargo.toml crates/breakpoint-server/Cargo.toml
COPY crates/breakpoint-client/Cargo.toml crates/breakpoint-client/Cargo.toml
COPY crates/breakpoint-relay/Cargo.toml crates/breakpoint-relay/Cargo.toml
COPY crates/games/breakpoint-golf/Cargo.toml crates/games/breakpoint-golf/Cargo.toml
COPY crates/games/breakpoint-platformer/Cargo.toml crates/games/breakpoint-platformer/Cargo.toml
COPY crates/games/breakpoint-lasertag/Cargo.toml crates/games/breakpoint-lasertag/Cargo.toml
COPY crates/adapters/breakpoint-github/Cargo.toml crates/adapters/breakpoint-github/Cargo.toml

# Copy vendored dependencies
COPY vendor/ vendor/

# Create stub lib.rs files so cargo can resolve the workspace
RUN mkdir -p crates/breakpoint-core/src && echo "" > crates/breakpoint-core/src/lib.rs && \
    mkdir -p crates/breakpoint-server/src && echo "fn main() {}" > crates/breakpoint-server/src/main.rs && \
    mkdir -p crates/breakpoint-client/src && echo "" > crates/breakpoint-client/src/lib.rs && \
    mkdir -p crates/breakpoint-relay/src && echo "fn main() {}" > crates/breakpoint-relay/src/main.rs && \
    mkdir -p crates/games/breakpoint-golf/src && echo "" > crates/games/breakpoint-golf/src/lib.rs && \
    mkdir -p crates/games/breakpoint-platformer/src && echo "" > crates/games/breakpoint-platformer/src/lib.rs && \
    mkdir -p crates/games/breakpoint-lasertag/src && echo "" > crates/games/breakpoint-lasertag/src/lib.rs && \
    mkdir -p crates/adapters/breakpoint-github/src && echo "" > crates/adapters/breakpoint-github/src/lib.rs

# Pre-build dependencies (cached layer — stubs may cause warnings, but deps are compiled)
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p breakpoint-server --features github-poller; exit 0

# Copy actual source code
COPY crates/ crates/

# Build server binary
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    cargo build --release -p breakpoint-server --features github-poller && \
    cp target/release/breakpoint-server /usr/local/bin/breakpoint-server

# Build WASM client
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/build/target \
    wasm-pack build crates/breakpoint-client --target web --out-dir /build/wasm-pkg

# ── Runtime ──────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    wget \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 breakpoint

WORKDIR /app

# Copy server binary
COPY --from=builder /usr/local/bin/breakpoint-server /app/breakpoint-server

# Copy web assets
COPY web/ /app/web/

# Copy WASM client bundle into web directory
COPY --from=builder /build/wasm-pkg/ /app/web/pkg/

# Copy default config if present
COPY breakpoint.toml* /app/

RUN chown -R breakpoint:breakpoint /app

USER breakpoint

EXPOSE 8080

ENV RUST_LOG=info

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:8080/api/v1/status || exit 1

ENTRYPOINT ["/app/breakpoint-server"]
