# Breakpoint Architecture

## System Overview

Breakpoint is a browser-based multiplayer gaming platform built in Rust, compiled to WebAssembly for the client. It uses a host-authoritative networking model where one browser acts as the game authority, with an Axum server handling relay, event ingestion, and static file serving.

```
                     ┌──────────────────────────────────────┐
                     │           Axum Server                │
                     │  ┌─────────┐  ┌──────┐  ┌────────┐  │
External ──REST/WH──►│  │ Events  │  │  WS  │  │  SSE   │  │
Systems              │  │  Store  │  │ Relay │  │ Stream │  │
                     │  └────┬────┘  └──┬───┘  └───┬────┘  │
                     │       │          │          │        │
                     │       └──────────┼──────────┘        │
                     │                  │                    │
                     │       ┌──────────┴──────────┐        │
                     │       │   Room Manager      │        │
                     │       └──────────┬──────────┘        │
                     └──────────────────┼───────────────────┘
                                        │ WSS
           ┌────────────────────────────┼────────────────────────┐
           │                            │                        │
    ┌──────┴──────┐              ┌──────┴──────┐          ┌──────┴──────┐
    │ Host Client │              │   Client 2  │          │   Client N  │
    │  (Authority)│              │  (Predicted) │          │  (Predicted) │
    │ ┌─────────┐ │              │ ┌─────────┐ │          │ ┌─────────┐ │
    │ │  Game   │ │              │ │  Game   │ │          │ │  Game   │ │
    │ │  Sim    │ │              │ │  View   │ │          │ │  View   │ │
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

Axum binary serving as the game relay and event hub:

- **`api.rs`** — REST endpoints: `POST /api/v1/events`, `POST /api/v1/events/:id/claim`, `GET /api/v1/status`
- **`sse.rs`** — `GET /api/v1/events/stream` — Server-Sent Events for real-time alert streaming
- **`webhooks/github.rs`** — `POST /api/v1/webhooks/github` — GitHub webhook transformer
- **`ws.rs`** — WebSocket handler for game state relay
- **`room_manager.rs`** — Room lifecycle (create, join, leave, broadcast)
- **`event_store.rs`** — In-memory event store with broadcast channel
- **`auth.rs`** — Bearer token auth + GitHub HMAC signature verification
- **`config.rs`** — TOML config file loading with env var overrides

### breakpoint-client

WASM library (`cdylib` + `rlib`) entry point via `wasm-bindgen`:

- **`lobby.rs`** — Lobby UI (create/join rooms, game selection, ready system)
- **`game/`** — Game rendering and input handling per game type
- **`overlay.rs`** — Full overlay system (ticker, toasts, dashboard, claim UI)
- **`net_client.rs`** — WebSocket client connection
- **`settings.rs`** — Settings panel (audio, overlay preferences)
- **`audio.rs`** — Sound effects with per-priority volume
- **`camera.rs`** — Bevy camera setup
- **`editor.rs`** — Course editor for mini-golf

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

1. Client sends `Input` message with serialized player input
2. Server relays to host client
3. Host runs authoritative game simulation
4. Host sends `GameState` message with serialized state
5. Server broadcasts to all clients in the room
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

### Direct Host

Host browser runs the game server. Other players connect via WebSocket to the host's machine. Works on LAN or VPN without additional infrastructure.

### Relay

The `breakpoint-relay` crate provides a stateless WebSocket relay. Clients connect to the relay, which forwards messages between host and clients. Enables NAT traversal without exposing host machines.

### Hybrid

The Axum server runs persistently, serving the WASM client, handling event ingestion, and relaying game traffic. Best for teams with internal infrastructure.

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust (Edition 2024) |
| Client Runtime | WebAssembly (wasm-bindgen, web-sys) |
| Game Framework | Bevy 0.18 |
| Server Framework | Axum 0.8 |
| Serialization | MessagePack (game state), JSON (API/events) |
| Build Tools | cargo, wasm-pack |
| CI/CD | GitHub Actions (self-hosted, Docker containers) |
