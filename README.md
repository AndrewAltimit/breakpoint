# Breakpoint

**Office Hours Gaming Platform with Agent Monitoring Overlay**

Breakpoint is a browser-based multiplayer gaming platform for **agentic office hours** — synchronous team sessions where humans stay socially connected while AI agents handle development tasks. Lightweight real-time games keep distributed teams engaged, paired with a unified overlay system that surfaces agent activity, alerts, and decision points directly into the shared gaming experience.

Built in Rust compiled to WebAssembly. Runs over WSS on port 443 for corporate network compatibility. No installs, no accounts — join via URL and play within seconds.

All code is authored by AI agents under human direction.

## Quick Start

### Docker (recommended)

```bash
docker run -p 8080:8080 -e BREAKPOINT_API_TOKEN=your-secret ghcr.io/andrewaltimit/breakpoint:latest
```

Open `http://localhost:8080` in your browser.

### From Source

```bash
# Build server + WASM client
cargo build --release -p breakpoint-server
wasm-pack build crates/breakpoint-client --target web --out-dir ../../web/pkg

# Run
BREAKPOINT_API_TOKEN=your-secret ./target/release/breakpoint-server
```

### Docker Compose

```bash
cd examples/
cp .env.example .env
# Edit .env with your API token
docker compose up -d
```

## Games

| Game | Players | Description |
|------|---------|-------------|
| Simultaneous Mini-Golf | 2-8 | All players putt simultaneously. First to sink earns bonus points. |
| Platform Racer | 2-6 | Race through procedural obstacle courses. |
| Laser Tag Arena | 2-8 | Top-down arena with reflective walls and power-ups. |

Games are pluggable modules implementing the `BreakpointGame` trait. Adding a new game requires no changes to networking, overlay, or server code. See [docs/GAME-DEVELOPMENT.md](docs/GAME-DEVELOPMENT.md).

## Alert Overlay

The overlay surfaces agent activity from any system that can POST JSON:

| Priority | Behavior | Examples |
|----------|----------|---------|
| **Ambient** | Scrolling ticker, no interruption | Task completed, tests passed, PR merged |
| **Notice** | Toast notification, auto-dismiss 8s | Build failed, review requested |
| **Urgent** | Persistent banner until acknowledged | Deploy awaiting approval, agent blocked |
| **Critical** | Game pause + modal overlay | Production incident, security alert |

Events are claimable — any player can click "Handle" and all others see "Handled by [name]".

### Send an Event

```bash
curl -X POST http://localhost:8080/api/v1/events \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "evt-001",
    "event_type": "pipeline.failed",
    "source": "github-actions",
    "priority": "notice",
    "title": "CI failed on main",
    "timestamp": "2026-02-12T14:32:00Z",
    "action_required": true
  }'
```

See [docs/INTEGRATION-GUIDE.md](docs/INTEGRATION-GUIDE.md) for the full event schema, SSE streaming, GitHub webhooks, and example adapters in Python, Node.js, and shell.

## Architecture

```
Axum Server (Rust)
├── Game Loop (Authority)        ← Server-authoritative game simulation
├── Room Manager                 ← Room lifecycle, player management
├── REST API                     ← Event ingestion, SSE alerts
└── WebSocket Handler            ← Game state broadcast to all clients

All Clients (WASM in browser)
├── Game View                   ← Renders server-authoritative state
└── Alert Overlay               ← Ambient ticker, toasts, dashboard, claim system
```

**Deployment modes:** Server (direct), relay (NAT traversal), or Docker. See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md).

## Project Structure

```
breakpoint/
├── crates/
│   ├── breakpoint-core/              # Shared types, traits, event schema
│   ├── breakpoint-server/            # Axum server (game authority)
│   ├── breakpoint-client/            # WASM browser client (Bevy)
│   ├── breakpoint-relay/             # Stateless WS relay for NAT traversal
│   ├── games/
│   │   ├── breakpoint-golf/          # Simultaneous mini-golf
│   │   ├── breakpoint-platformer/    # Platform racer
│   │   └── breakpoint-lasertag/      # Laser tag arena
│   └── adapters/
│       └── breakpoint-github/        # GitHub Actions polling + agent detection
├── web/                              # Static assets (HTML, CSS, sprites, sounds)
├── docker/                           # Dockerfiles (CI + production)
├── docs/                             # Architecture, integration, deployment guides
├── examples/                         # Docker Compose, adapter scripts, workflow templates
└── .github/                          # GitHub Actions workflows
```

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the full system design.

## Development

```bash
cargo check                                                    # Compile check
cargo fmt --all                                                # Format
cargo clippy --workspace --all-targets -- -D warnings          # Lint
cargo test --workspace                                         # Test
cargo build --workspace --release                              # Build
wasm-pack build crates/breakpoint-client --target web          # WASM

# Containerized CI (matches GitHub Actions)
docker compose --profile ci run --rm rust-ci cargo test --workspace
```

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (Edition 2024) |
| Client | WebAssembly (wasm-bindgen, web-sys, Bevy 0.18) |
| Server | Axum 0.8, Tokio |
| Serialization | MessagePack (game state), JSON (API/events) |
| CI/CD | GitHub Actions (self-hosted runner, Docker containers) |

## Documentation

- [Architecture](docs/ARCHITECTURE.md) — System design, data flow, protocol
- [Integration Guide](docs/INTEGRATION-GUIDE.md) — Event schema, API endpoints, adapters
- [Game Development](docs/GAME-DEVELOPMENT.md) — BreakpointGame trait, adding new games
- [Deployment](docs/DEPLOYMENT.md) — Docker, relay, TLS, corporate networks
- [Design Document](BREAKPOINT-DESIGN-DOC.md) — Full specification

## License

Dual-licensed under [Unlicense](LICENSE) and [MIT](LICENSE-MIT).
