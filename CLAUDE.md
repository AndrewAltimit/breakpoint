# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Breakpoint is a browser-based multiplayer gaming platform (Rust + WASM) designed for agentic office hours. It provides real-time games over WSS with an alert overlay that surfaces agent activity from external systems (GitHub, CI/CD, custom agents). See `BREAKPOINT-DESIGN-DOC.md` for the full specification.

All code is authored by AI agents under human direction. No external contributions are accepted.

## Build Commands

```bash
# Check workspace compiles
cargo check

# Format (edition 2024, max_width=100)
cargo fmt --all
cargo fmt --all -- --check

# Lint (CI runs with -D warnings, so all warnings are errors)
cargo clippy --workspace --all-targets -- -D warnings

# Test
cargo test --workspace

# Build release
cargo build --workspace --release

# Build single crate
cargo build -p breakpoint-core
cargo build -p breakpoint-server
cargo build -p breakpoint-client

# Run a single test by name
cargo test -p breakpoint-core test_name

# License/advisory checks
cargo deny check

# WASM client build
wasm-pack build crates/breakpoint-client --target web --out-dir ../../web/pkg
```

### Containerized CI (matches what GitHub Actions runs)

```bash
docker compose --profile ci run --rm rust-ci cargo fmt --all -- --check
docker compose --profile ci run --rm rust-ci cargo clippy --workspace --all-targets -- -D warnings
docker compose --profile ci run --rm rust-ci cargo test --workspace
```

## Architecture

**Workspace layout** — Three crates in `crates/`:

- **breakpoint-core** — Shared types with no runtime dependencies. Everything that both server and client need: event schema (`events.rs`), the `BreakpointGame` trait (`game_trait.rs`), player/room types, network message types (`net/`), and overlay data models (`overlay/`). Game crates and adapter crates will also depend on this.
- **breakpoint-server** — Axum binary. Will handle: WSS game state relay, REST event ingestion (`/api/v1/events`), SSE event streaming, webhook adapters (GitHub etc.), room management, static file serving for the WASM client.
- **breakpoint-client** — WASM library (`crate-type = ["cdylib", "rlib"]`). Entry point via `wasm-bindgen`. Will handle: game rendering (Canvas2D/wgpu), local simulation with server reconciliation, overlay rendering, WSS connection, lobby UI.

**Future crates** (per design doc, not yet created):
- `crates/games/breakpoint-golf/`, `breakpoint-platformer/`, `breakpoint-lasertag/` — Each implements `BreakpointGame` trait
- `crates/adapters/breakpoint-github/` — GitHub webhook transformer + Actions polling

**Key design patterns:**
- Host-authoritative client-server: host browser runs authoritative simulation, clients send inputs and receive state
- Dual-channel communication: game state over WSS (MessagePack, tick-aligned), alerts over SSE/WSS (JSON, event-driven)
- Games are pluggable via the `BreakpointGame` trait — networking, overlay, and lobby code don't change when adding games
- Alert overlay operates independently of game lifecycle (works in lobby, between rounds, during gameplay)

**Static assets** live in `web/` (HTML shell, CSS, sprites, sounds). The server serves these and the WASM bundle.

## Rust Conventions

- **Edition 2024**, MSRV 1.91.0
- Workspace lints: `clone_on_ref_ptr`, `dbg_macro`, `todo`, `unimplemented` all warn; `unsafe_op_in_unsafe_fn` warns
- Clippy thresholds: cognitive-complexity 25, too-many-lines 100, too-many-arguments 7
- Formatting: 100 char max width, 4-space indent, Unix newlines, tall fn params
- Shared dependencies declared in `[workspace.dependencies]` and referenced with `.workspace = true`
- Dual license: MIT OR Apache-2.0

## CI/CD Pipeline

Three GitHub Actions workflows, all on a self-hosted runner using Docker containers:

- **ci.yml** — push to main: fmt, clippy, test, build, cargo-deny
- **pr-validation.yml** — PRs: same CI + Gemini/Codex AI reviews + agent auto-fix (up to 5 iterations). Agent infrastructure uses `github-agents` and `automation-cli` binaries from template-repo (degrades gracefully if missing).
- **main-ci.yml** — push to main + `v*` tags: CI + release build (server binary + WASM bundle) + GitHub Release creation

## Docker

- `docker/rust-ci.Dockerfile` — Rust stable + wasm-pack + cargo-deny. Used by `docker compose --profile ci`.
- `docker-compose.yml` — `rust-ci` service for CI, plus 9 MCP services (code-quality, gemini, codex, etc.) under `--profile services` for interactive agent sessions. MCP images are pre-built from template-repo, not buildable from this repo.
