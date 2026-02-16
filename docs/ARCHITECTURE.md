# Breakpoint Architecture

## System Overview

Breakpoint is a browser-based multiplayer gaming platform built in Rust, compiled to WebAssembly for the client. It uses a server-authoritative networking model where the Axum server runs the game simulation, and all browser clients are equal renderers that send inputs and receive state.

```
                     ┌──────────────────────────────────────┐
                     │           Axum Server                │
                     │  ┌─────────┐  ┌──────┐  ┌────────┐  │
External ──REST/WH──►│  │ Events  │  │  WS  │  │  SSE   │  │
Systems              │  │  Store  │  │Handler│  │ Stream │  │
                     │  └────┬────┘  └──┬───┘  └───┬────┘  │
                     │       │          │          │        │
                     │       └──────────┼──────────┘        │
                     │                  │                    │
                     │       ┌──────────┴──────────┐        │
                     │       │   Room Manager      │        │
                     │       │   + Game Loop       │        │
                     │       │   (Authority)       │        │
                     │       └──────────┬──────────┘        │
                     └──────────────────┼───────────────────┘
                                        │ WSS
           ┌────────────────────────────┼────────────────────────┐
           │                            │                        │
    ┌──────┴──────┐              ┌──────┴──────┐          ┌──────┴──────┐
    │  Client 1   │              │   Client 2  │          │   Client N  │
    │  (Renderer) │              │  (Renderer)  │          │  (Renderer)  │
    │ ┌─────────┐ │              │ ┌─────────┐ │          │ ┌─────────┐ │
    │ │  Game   │ │              │ │  Game   │ │          │ │  Game   │ │
    │ │  View   │ │              │ │  View   │ │          │ │  View   │ │
    │ ├─────────┤ │              │ ├─────────┤ │          │ ├─────────┤ │
    │ │ Overlay │ │              │ │ Overlay │ │          │ │ Overlay │ │
    │ └─────────┘ │              │ └─────────┘ │          │ └─────────┘ │
    └─────────────┘              └─────────────┘          └─────────────┘
```

## Workspace Crates

### breakpoint-core

Shared types with no runtime dependencies. Everything that both server and client need:

- **`events.rs`** — `Event`, `EventType`, `Priority` — the canonical event schema
- **`game_trait.rs`** — `BreakpointGame` trait that all games implement
- **`player.rs`** — `Player`, `PlayerId` types
- **`room.rs`** — `RoomConfig`, `RoomState` for room management
- **`net/messages.rs`** — All network message types (Join, Leave, GameState, Input, AlertEvent, etc.)
- **`net/protocol.rs`** — MessagePack serialization with 1-byte type prefix
- **`overlay/`** — Overlay data models (config, dashboard, alert tiers)

### breakpoint-server

Axum binary running the server-authoritative game simulation, event hub, and WebSocket broadcast:

- **`api.rs`** — REST endpoints: `POST /api/v1/events`, `POST /api/v1/events/:id/claim`, `GET /api/v1/status`
- **`sse.rs`** — `GET /api/v1/events/stream` — Server-Sent Events for real-time alert streaming
- **`webhooks/github.rs`** — `POST /api/v1/webhooks/github` — GitHub webhook transformer
- **`ws.rs`** — WebSocket handler for client connections and input routing
- **`game_loop.rs`** — Server-authoritative game tick loop (runs `BreakpointGame` instances)
- **`room_manager.rs`** — Room lifecycle (create, join, leave, game start/stop)
- **`event_store.rs`** — In-memory event store with broadcast channel
- **`auth.rs`** — Bearer token auth + GitHub HMAC signature verification
- **`config.rs`** — TOML config file loading with env var overrides

### breakpoint-client

WASM library (`cdylib` + `rlib`) entry point via `wasm-bindgen`. Uses a custom WebGL2 renderer (not a game framework) with an HTML/CSS/JS UI layer:

- **`app.rs`** — Application state machine + requestAnimationFrame loop (`Rc<RefCell<App>>` pattern)
- **`renderer.rs`** — WebGL2 renderer with 4 GLSL shader programs (unlit, gradient, ripple, glow)
- **`scene.rs`** — Flat scene graph (`Vec<RenderObject>`) rebuilt each frame
- **`bridge.rs`** — JS↔Rust bridge: pushes UI state via `window._breakpointUpdate()`, receives callbacks via globals
- **`camera_gl.rs`** — Perspective camera with game-specific modes (GolfFollow, PlatformerFollow, LaserTagFixed)
- **`game/`** — Per-game rendering (`*_render.rs`) and input handling (`*_input.rs`)
- **`overlay.rs`** — Alert overlay state management
- **`net_client.rs`** — WebSocket client connection
- **`audio.rs`** — Sound effects with per-priority volume
- **`input.rs`** — Keyboard + mouse input tracking
- **`theme.rs`** — Theming system (colors, game-specific themes, loaded from `theme.json`)
- **`shaders_gl/`** — GLSL vertex + fragment shaders

UI elements (lobby, HUD, overlay, settings, between-rounds, game-over) are implemented in `web/index.html`, `web/style.css`, and `web/ui.js`.

### breakpoint-relay

Stateless WebSocket relay for NAT traversal:

- **`relay.rs`** — Room state management, message forwarding
- **`main.rs`** — Axum server with `/relay` WebSocket endpoint

### Game Crates (`crates/games/`)

- **breakpoint-golf** — Simultaneous mini-golf (2-8 players, 10 Hz)
- **breakpoint-platformer** — Platform racer (2-6 players, 15 Hz)
- **breakpoint-lasertag** — Top-down laser tag arena (2-8 players, 20 Hz)

### Adapter Crates (`crates/adapters/`)

- **breakpoint-github** — GitHub Actions polling monitor with agent/bot detection

## Data Flow

### Game State (WSS, MessagePack)

1. Client sends `PlayerInput` message with serialized player input
2. Server's `game_loop` receives input via `GameCommand::PlayerInput`
3. Server runs authoritative game simulation (`BreakpointGame::update()`)
4. Server serializes state via `BreakpointGame::serialize_state()`
5. Server broadcasts `GameState` message to all clients in the room
6. Clients apply state and render

### Alert Events (REST + SSE/WSS, JSON)

1. External system POSTs event to `/api/v1/events` (or GitHub webhook hits `/api/v1/webhooks/github`)
2. Server stores event in `EventStore` and broadcasts via channel
3. SSE clients receive event as `alert` SSE event
4. Background task encodes event as `AlertEvent` ServerMessage and broadcasts to all WSS rooms
5. Client overlay renders based on priority tier (ambient ticker, toast, banner, or modal)

### Event Claiming

1. Player clicks "Handle" on an event in the overlay
2. Client sends claim via REST `POST /api/v1/events/:id/claim`
3. Server marks event as claimed in EventStore
4. Claim broadcast reaches all connected clients
5. Other players see "Handled by [name]"

## Network Protocol

All game messages use MessagePack with a 1-byte type prefix:

| Byte | Message Type | Direction |
|------|-------------|-----------|
| 0x01 | JoinRoom | Client -> Server |
| 0x02 | LeaveRoom | Client -> Server |
| 0x10 | GameState | Server -> Client |
| 0x11 | PlayerInput | Client -> Server |
| 0x12 | GameStart | Server -> Client |
| 0x13 | GameEnd | Server -> Client |
| 0x20 | AlertEvent | Server -> Client |
| 0x21 | ClaimEvent | Bidirectional |
| 0x22 | Ping/Pong | Bidirectional |
| 0x23 | OverlayConfig | Bidirectional |

## Deployment Modes

### Server (Primary)

The Axum server runs the authoritative game simulation and serves the WASM client. All players connect as equal clients via WebSocket. Best for teams with internal infrastructure or Docker deployment.

### Relay

The `breakpoint-relay` crate provides a stateless WebSocket relay for NAT traversal. Clients connect to the relay, which forwards messages between the server and clients. Enables deployment without exposing the server directly.

### Docker

The production Docker image bundles the server binary, WASM client, and static assets. One command deployment via `docker run` or `docker compose`.

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (Edition 2024) |
| Client Runtime | WebAssembly (wasm-bindgen, web-sys) |
| Rendering | Custom WebGL2 (4 GLSL shaders, flat scene graph) |
| UI Layer | HTML/CSS/JS (lobby, HUD, overlay) |
| Server Framework | Axum 0.8 |
| Serialization | MessagePack (game state), JSON (API/events) |
| Build Tools | cargo, wasm-pack |
| CI/CD | GitHub Actions (self-hosted, Docker containers) |
