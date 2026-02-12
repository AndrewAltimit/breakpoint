# syntax=docker/dockerfile:1.4
# Rust CI image for Breakpoint
# Stable toolchain with wasm-pack for WASM builds

FROM rust:1.93-slim

# System dependencies
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    git \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install toolchain components
RUN rustup component add rustfmt clippy

# Install cargo-deny for license/advisory checks
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install cargo-deny --locked 2>/dev/null || true

# Install wasm-pack for WASM builds
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    cargo install wasm-pack --locked 2>/dev/null || true

# Non-root user (overridden by docker-compose USER_ID/GROUP_ID)
RUN useradd -m -u 1000 ciuser \
    && mkdir -p /tmp/cargo && chmod 1777 /tmp/cargo

WORKDIR /workspace

ENV CARGO_HOME=/tmp/cargo
ENV RUSTUP_HOME=/usr/local/rustup
ENV CARGO_INCREMENTAL=1 \
    CARGO_NET_RETRY=10 \
    RUST_BACKTRACE=short

CMD ["bash"]
