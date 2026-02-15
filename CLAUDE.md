# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Breakpoint is a browser-based multiplayer gaming platform (Rust + WASM) designed for agentic office hours. It provides real-time games over WSS with an alert overlay that surfaces agent activity from external systems (GitHub, CI/CD, custom agents). See `BREAKPOINT-DESIGN-DOC.md` for the full specification.

All code is authored by AI agents under human direction. No external contributions are accepted.

**Status:** Feature-complete (Phases 1–4). 467 tests pass across 8 workspace crates. Production-hardened with input validation, state machine enforcement, idle room cleanup, and event batch limits. Browser integration tests via Playwright (12 spec files, Chromium + Firefox).

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
cargo build -p breakpoint-relay
cargo build -p breakpoint-github

# Run a single test by name
cargo test -p breakpoint-core test_name

# License/advisory checks
cargo deny check

# WASM client build
wasm-pack build crates/breakpoint-client --target web --out-dir ../../web/pkg

# Server with GitHub polling feature
cargo build -p breakpoint-server --features github-poller
```

### Containerized CI (matches what GitHub Actions runs)

```bash
docker compose --profile ci run --rm rust-ci cargo fmt --all -- --check
docker compose --profile ci run --rm rust-ci cargo clippy --workspace --all-targets -- -D warnings
docker compose --profile ci run --rm rust-ci cargo test --workspace
```

### Docker production build

```bash
docker build -f docker/server.Dockerfile -t breakpoint .
```

## Dependency Patching

Winit 0.30.12 is patched at build time via [patch-crate](https://crates.io/crates/patch-crate). The patch file lives in `patches/winit+0.30.12.patch` and fixes a DPR scaling bug in the WASM ResizeObserver. Run `cargo patch-crate` before any build command to apply patches (CI workflows do this automatically).

## Architecture

**Workspace layout** — Eight crates in `crates/`:

- **breakpoint-core** — Shared types with no runtime dependencies. Event schema (`events.rs`), `BreakpointGame` trait (`game_trait.rs`), player/room types, network message types (`net/`), overlay data models (`overlay/` including config, ticker, toast, dashboard).
- **breakpoint-server** — Axum binary. Server-authoritative game simulation (`game_loop.rs`), WSS game state broadcast, REST event ingestion (`/api/v1/events`), SSE streaming, GitHub webhook adapter, room management, TOML config loading, static file serving. Optional `github-poller` feature flag spawns the GitHub Actions polling monitor.
- **breakpoint-client** — WASM library (`crate-type = ["cdylib", "rlib"]`), Bevy 0.18 game engine. Lobby UI, game rendering (golf/platformer/lasertag), overlay system (ticker/toasts/dashboard/claim), settings panel with localStorage persistence, audio, course editor.
- **breakpoint-relay** — Stateless WebSocket relay for NAT traversal. Protocol-agnostic message forwarding, room code generation, auto-cleanup.
- **breakpoint-golf** — Simultaneous mini-golf (2-8 players, 10 Hz). Physics, obstacles, scoring.
- **breakpoint-platformer** — Platform racer (2-6 players, 15 Hz). Procedural courses, race/survival modes, power-ups.
- **breakpoint-lasertag** — Laser tag arena (2-8 players, 20 Hz). Reflective walls, FFA/team modes, power-ups.
- **breakpoint-github** — GitHub Actions polling adapter with agent/bot detection. Configurable glob-style patterns.

**Key design patterns:**
- Server-authoritative: the Axum server runs the game simulation (`game_loop.rs`), all clients are equal renderers that send inputs and receive state
- Dual-channel communication: game state over WSS (MessagePack, tick-aligned), alerts over SSE/WSS (JSON, event-driven)
- Games are pluggable via the `BreakpointGame` trait — networking, overlay, and lobby code don't change when adding games
- Alert overlay operates independently of game lifecycle (works in lobby, between rounds, during gameplay)
- Game crates behind feature flags for optional compilation (reduce WASM bundle)
- Server config via TOML file (`breakpoint.toml`) with env var overrides

**Static assets** live in `web/` (HTML shell, CSS, sprites, sounds). The server serves these and the WASM bundle.

## Rust Conventions

- **Edition 2024**, MSRV 1.91.0
- Workspace lints: `clone_on_ref_ptr`, `dbg_macro`, `todo`, `unimplemented` all warn; `unsafe_op_in_unsafe_fn` warns
- Clippy thresholds: cognitive-complexity 25, too-many-lines 100, too-many-arguments 7
- Formatting: 100 char max width, 4-space indent, Unix newlines, tall fn params
- Shared dependencies declared in `[workspace.dependencies]` and referenced with `.workspace = true`
- Dual license: Unlicense OR MIT
- Release profile: `opt-level = "z"`, LTO, `codegen-units = 1`, strip

## CI/CD Pipeline

Three GitHub Actions workflows, all on a self-hosted runner using Docker containers:

- **ci.yml** — push to main: fmt, clippy, test, build, cargo-deny
- **pr-validation.yml** — PRs: same CI + Gemini/Codex AI reviews + agent auto-fix (up to 5 iterations). Agent infrastructure uses `github-agents` and `automation-cli` binaries from template-repo (degrades gracefully if missing).
- **main-ci.yml** — push to main + `v*` tags: CI + matrix release builds (Linux x86_64 + aarch64) + Docker image push to GHCR + GitHub Release creation

## Docker

- `docker/rust-ci.Dockerfile` — Rust stable + wasm-pack + cargo-deny + patch-crate. Used by `docker compose --profile ci`.
- `docker/server.Dockerfile` — Multi-stage production image. Builder compiles server binary + WASM client. Runtime is `debian:bookworm-slim` with just the binary, web assets, and WASM bundle. Exposes port 8080.
- `docker-compose.yml` — `rust-ci` service for CI, plus 9 MCP services (code-quality, gemini, codex, etc.) under `--profile services` for interactive agent sessions. MCP images are pre-built from template-repo, not buildable from this repo.
- `examples/docker-compose.yml` — Production deployment compose file.

## Key File Paths

| Purpose | Path |
|---------|------|
| Event schema | `crates/breakpoint-core/src/events.rs` |
| Game trait | `crates/breakpoint-core/src/game_trait.rs` |
| Network protocol | `crates/breakpoint-core/src/net/protocol.rs` |
| Message types | `crates/breakpoint-core/src/net/messages.rs` |
| Overlay config types | `crates/breakpoint-core/src/overlay/config.rs` |
| Server entry point | `crates/breakpoint-server/src/main.rs` |
| Server game loop | `crates/breakpoint-server/src/game_loop.rs` |
| Room manager | `crates/breakpoint-server/src/room_manager.rs` |
| Server config loading | `crates/breakpoint-server/src/config.rs` |
| REST API handlers | `crates/breakpoint-server/src/api.rs` |
| WebSocket handler | `crates/breakpoint-server/src/ws.rs` |
| Auth (tokens + HMAC) | `crates/breakpoint-server/src/auth.rs` |
| Event store | `crates/breakpoint-server/src/event_store.rs` |
| GitHub webhook | `crates/breakpoint-server/src/webhooks/github.rs` |
| Client entry point | `crates/breakpoint-client/src/lib.rs` |
| Lobby UI | `crates/breakpoint-client/src/lobby.rs` |
| Network client | `crates/breakpoint-client/src/net_client.rs` |
| Overlay rendering | `crates/breakpoint-client/src/overlay.rs` |
| Camera setup | `crates/breakpoint-client/src/camera.rs` |
| Settings UI | `crates/breakpoint-client/src/settings.rs` |
| Agent detection | `crates/adapters/breakpoint-github/src/agent_detect.rs` |
| Relay server | `crates/breakpoint-relay/src/relay.rs` |
