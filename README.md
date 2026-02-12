# Breakpoint

**Office Hours Gaming Platform with Agent Monitoring Overlay**

Breakpoint is an open-source, browser-based multiplayer gaming platform for **agentic office hours** — synchronous team sessions where humans stay socially connected and available while AI agents handle development tasks. It provides lightweight real-time multiplayer games that keep distributed teams engaged, paired with a unified overlay system that surfaces agent activity, alerts, and decision points directly into the shared gaming experience.

Built in Rust compiled to WebAssembly. Runs over WSS on port 443 for corporate network compatibility. No installs, no accounts — join via URL and play within seconds.

## Architecture

```
Host Machine
├── Axum Server (Rust)           ← WSS game state, REST event ingestion, SSE alerts
├── Game Engine (Authority)      ← Authoritative game simulation
└── WASM Client (Host Player)    ← Renders game + overlay

Player Clients (WASM in browser)
├── Game View                    ← Local simulation with server reconciliation
└── Alert Overlay                ← Ambient ticker, toasts, dashboard, claim system
```

**Deployment modes:** Direct host (LAN/VPN), relay (NAT traversal), or hybrid (persistent internal service).

## Games

| Game | Players | Description |
|------|---------|-------------|
| Simultaneous Mini-Golf | 2-8 | All players putt simultaneously. First to sink earns bonus points. 10 Hz state sync. |
| Platform Racer | 2-6 | Race or survive through procedural obstacle courses. 15-20 Hz state sync. |
| Laser Tag Arena | 2-8 | Top-down arena with reflective walls and power-ups. Corporate-friendly neon aesthetic. |

Games are pluggable modules implementing the `BreakpointGame` trait. Adding a new game requires no changes to networking, overlay, or server code.

## Alert Overlay

The overlay surfaces agent activity from any system that can POST JSON:

| Tier | Behavior | Examples |
|------|----------|---------|
| **Ambient** | Scrolling ticker, no interruption | Task completed, tests passed, PR merged |
| **Notice** | Toast notification, auto-dismiss 8s | Build failed, review requested |
| **Urgent** | Persistent banner until acknowledged | Deploy awaiting approval, agent blocked |
| **Critical** | Game pause + modal overlay | Production incident, security alert |

Events are claimable — any player can click "Handle" and all others see "Handled by [name]".

## Project Structure

```
breakpoint/
├── Cargo.toml                     # Workspace root
├── crates/
│   ├── breakpoint-core/           # Shared types, traits, event schema
│   ├── breakpoint-server/         # Axum host server
│   └── breakpoint-client/         # WASM browser client
├── web/                           # Static assets (HTML, CSS, sprites, sounds)
├── ci/                            # CI test infrastructure
├── docker/                        # Docker images (rust-ci)
├── tools/                         # CLI scripts for agent orchestration
└── .github/                       # GitHub Actions workflows
```

Future crates (added as implementation progresses):
- `crates/games/breakpoint-golf/` — Mini-golf game module
- `crates/games/breakpoint-platformer/` — Platform racer module
- `crates/games/breakpoint-lasertag/` — Laser tag module
- `crates/adapters/breakpoint-github/` — GitHub webhook + polling adapter

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Core Language | Rust |
| Client Runtime | WASM (wasm-bindgen, web-sys) |
| Game Framework | Bevy or custom Canvas2D |
| Networking | WSS (tokio-tungstenite / web-sys) |
| Host Server | Axum |
| Serialization | serde + MessagePack (game state), JSON (API) |
| Build | cargo + wasm-pack + trunk |
| CI/CD | GitHub Actions (self-hosted runner) |

## Development

### Prerequisites

- Rust stable toolchain
- `wasm-pack` for WASM builds: `cargo install wasm-pack`
- Docker + Docker Compose (for containerized CI)

### Building

```bash
# Check the workspace
cargo check

# Run tests
cargo test

# Build the server
cargo build --release -p breakpoint-server

# Build the WASM client
wasm-pack build crates/breakpoint-client --target web --out-dir ../../web/pkg
```

### Running locally

```bash
# Start the server (serves WASM client + handles WSS/REST/SSE)
cargo run -p breakpoint-server
```

## CI/CD

All CI runs on a self-hosted GitHub Actions runner. Rust compilation and testing execute inside Docker containers for reproducibility; AI agent tooling runs directly on the host where it is pre-installed.

| Workflow | Trigger | Purpose |
|----------|---------|---------|
| `ci.yml` | push to main | CI: fmt, clippy, test, build, cargo-deny |
| `pr-validation.yml` | pull request | Full PR pipeline: CI + Gemini/Codex AI reviews + agent auto-fix |
| `main-ci.yml` | push to main, `v*` tags | CI on main, build release artifacts and create GitHub Release on tags |

### CI Stages

All stages run inside the `rust-ci` Docker container (`docker compose --profile ci`):

1. **Format check** — `cargo fmt --check`
2. **Clippy** — `cargo clippy --workspace -- -D warnings`
3. **Unit tests** — `cargo test --workspace`
4. **Build** — release build of server + WASM client
5. **cargo-deny** — license and advisory checks

### PR Review Pipeline

PRs receive automated AI code reviews from Gemini and Codex, followed by an agent that can automatically apply fixes from review feedback (with a 5-iteration safety limit per agent type). If CI stages fail, a separate failure-handler agent attempts automated fixes.

### Runner Dependencies from template-repo

The self-hosted runner provides the following binaries built from [template-repo](https://github.com/AndrewAltimit/template-repo). These are expected to be on `PATH`; workflows degrade gracefully if they are missing.

| Binary | Source | Used By | Purpose |
|--------|--------|---------|---------|
| `github-agents` | `tools/rust/github-agents-cli` | `pr-validation.yml` | PR reviews (Gemini/Codex), iteration tracking |
| `automation-cli` | `tools/rust/automation-cli` | `pr-validation.yml` | Agent review response, failure handler |

### Secrets

| Secret | Required By | Purpose |
|--------|-------------|---------|
| `GITHUB_TOKEN` | all workflows | Standard GitHub token (automatic) |
| `AGENT_TOKEN` | `pr-validation.yml` | Personal access token for agent commits (write access) |
| `GOOGLE_API_KEY` | `pr-validation.yml` | Gemini API key for AI code reviews |
| `GEMINI_API_KEY` | `pr-validation.yml` | Gemini API key (alternative) |

### Release Pipeline

Tagging a commit with `v*` (e.g., `v0.1.0`) triggers a release build:

1. Full CI validation
2. Build release server binary + WASM client bundle
3. GitHub Release creation with artifacts attached and auto-generated changelog

## Docker Images

| Image | Dockerfile | Purpose |
|-------|-----------|---------|
| `rust-ci` | `docker/rust-ci.Dockerfile` | Rust CI with wasm-pack for WASM builds |

The repo also uses pre-built MCP images from [template-repo](https://github.com/AndrewAltimit/template-repo) for AI agent tooling (Claude Code, Gemini, Codex, etc.). These are referenced as `template-repo-mcp-<name>:latest` in `docker-compose.yml` and are **not buildable from this repo**. Build them once from a template-repo checkout:

```bash
cd /path/to/template-repo
docker compose --profile services build
```

CI workflows work without MCP images — they are only needed for interactive AI agent sessions.

## Data Integration

External systems feed events into Breakpoint via REST:

```bash
curl -X POST https://breakpoint.internal:8443/api/v1/events \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"event_type":"pipeline.failed","source":"github","title":"CI failed on main","timestamp":"2026-02-12T14:32:00Z"}'
```

See [BREAKPOINT-DESIGN-DOC.md](BREAKPOINT-DESIGN-DOC.md) for the full event schema, webhook adapter specification, and integration guide.

## License

Dual-licensed under MIT and Apache 2.0.
