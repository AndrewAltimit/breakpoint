# Changelog

## v0.1.0 — Initial Release

### Platform

- Browser-based multiplayer gaming over WebSocket
- Server-authoritative networking (Axum server runs game simulation)
- Room system with join codes, spectator mode, and late-join support
- Between-rounds scoreboard and game-over summary screens
- Course editor for mini-golf level creation

### Games

- **Simultaneous Mini-Golf** (2-8 players, 10 Hz) — all players putt at once, first to sink earns bonus
- **Platform Racer** (2-6 players, 15 Hz) — race through procedural obstacle courses
- **Laser Tag Arena** (2-8 players, 20 Hz) — top-down arena with reflective walls and power-ups

### Alert Overlay

- Four-tier priority system: Ambient (ticker), Notice (toast), Urgent (banner), Critical (modal)
- Event claiming — any player can handle an alert, visible to all
- Dashboard panel with event history and filtering (All / Agent / Human)
- Agent detection with robot badge on bot-generated events
- Configurable toast position and notification density
- Per-priority audio volume controls

### Server

- Axum server with REST event ingestion, SSE streaming, WebSocket game relay
- GitHub webhook adapter with HMAC signature verification
- GitHub Actions polling monitor with configurable agent/bot detection patterns
- TOML config file support with environment variable overrides
- Bearer token authentication for API endpoints

### Relay

- Stateless WebSocket relay server for NAT traversal
- Room code generation, automatic cleanup on disconnect

### Infrastructure

- Docker production image (multi-stage build)
- GitHub Actions CI/CD: fmt, clippy, test, build, cargo-deny, Docker push
- Release pipeline with artifact upload and GitHub Release creation
- WASM bundle optimization (opt-level z, LTO, strip)
- Game feature flags for optional compilation

### Documentation

- Architecture overview
- Integration guide with event schema reference
- Game development guide (BreakpointGame trait)
- Deployment guide (direct, relay, Docker, TLS)
- Example adapters (Python, Node.js, shell)
- Example GitHub Actions workflow for notifications
