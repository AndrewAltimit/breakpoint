# BREAKPOINT

**Office Hours Gaming Platform with Agent Monitoring Overlay**

Comprehensive Design Document — Version 1.0 — February 2026

Open Source • Rust + WASM • Real-Time Multiplayer • Agent-Aware

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Vision and Problem Statement](#2-vision-and-problem-statement)
3. [System Architecture](#3-system-architecture)
4. [Game Designs](#4-game-designs)
5. [Alert Overlay System](#5-alert-overlay-system)
6. [Data Integration Interface](#6-data-integration-interface)
7. [Reference Implementation: GitHub Integration](#7-reference-implementation-github-integration)
8. [Lobby and Room System](#8-lobby-and-room-system)
9. [Technology Stack](#9-technology-stack)
10. [Project Structure](#10-project-structure)
11. [Game Trait Interface](#11-game-trait-interface)
12. [Network Protocol](#12-network-protocol)
13. [Development Roadmap](#13-development-roadmap)
14. [Security Considerations](#14-security-considerations)
15. [Open Source Strategy](#15-open-source-strategy)
16. [Success Metrics](#16-success-metrics)
17. [Future Considerations](#17-future-considerations)

---

## 1. Executive Summary

Breakpoint is an open-source, browser-based multiplayer gaming platform designed for the emerging paradigm of **agentic office hours** — synchronous team sessions where humans remain socially connected and available while autonomous AI agents handle routine development tasks. The platform serves dual purposes: it provides lightweight, real-time multiplayer games that keep distributed teams engaged during monitoring windows, and it delivers a unified overlay system that surfaces agent activity, alerts, and decision points directly into the shared gaming experience.

The core insight is that as AI agents take over increasing portions of development workflows — writing code, reviewing PRs, managing CI/CD pipelines, triaging issues — human engineers still need to be present, reachable, and collectively aware of what the agents are doing. Traditional dashboards require context-switching and individual monitoring. Breakpoint solves this by making the shared game session the monitoring surface itself, where agent alerts interrupt or overlay gameplay when human attention is needed.

The platform is built in Rust compiled to WebAssembly for performance and safety, communicates over secure WebSockets (WSS) for corporate firewall compatibility, and exposes a standardized data integration interface that any GitOps or agent orchestration system can feed into. The reference implementation demonstrates monitoring of GitHub Actions pipelines, PR activity, issue boards, and repository changes, but the interface is designed to be tool-agnostic and extensible.

---

## 2. Vision and Problem Statement

### 2.1 The Agentic Office Hour

As AI coding agents mature, the role of the human engineer shifts from hands-on-keyboard execution to supervisory oversight, architectural decision-making, and approval of high-stakes changes. Teams are beginning to adopt "office hours" as a collaboration pattern: scheduled synchronous windows where the team is collectively online, agents are running, and humans are available for the 10–20% of tasks that require human judgment.

The problem is attention management. During the 80–90% of time when agents don't need humans, engineers drift to solo work, lose shared context, and become slow to respond when agents do need them. Existing monitoring tools (Slack notifications, dashboards, email alerts) are individual, asynchronous, and disconnected from the social fabric of the team.

### 2.2 The Breakpoint Solution

Breakpoint creates a shared attention surface that is engaging enough to keep the team present (via games) and informative enough to keep everyone aware of agent activity (via the overlay). The game is not a distraction from work — it is the work environment, augmented with the information streams that make supervision effective.

The metaphor is a mission control room where operators play cards between launches. The games are not the point — presence is the point. The games are the mechanism that makes presence sustainable and enjoyable over multi-hour windows.

### 2.3 Design Principles

- **Zero friction entry:** One person hosts, others join via URL. No installs, no accounts, no plugins. A new team member should be playing within 15 seconds of receiving the link.
- **Corporate-network friendly:** Everything runs over HTTPS/WSS on port 443. No UDP, no WebRTC, no firewall exceptions needed. Works behind corporate proxies and VPNs without configuration.
- **Simultaneous play:** All players act in parallel during rounds, not sequentially. Games feel energetic and social, like a shared experience rather than taking turns watching someone else play.
- **Agent-aware by default:** The overlay system is a first-class citizen, not an afterthought. Games are designed to coexist with alert interruptions gracefully — gameplay pauses and resumes cleanly when critical alerts demand attention.
- **Open and extensible:** The data integration interface is standardized. The reference implementation uses GitHub, but any system that can POST JSON can feed the overlay. Adding new games requires implementing a single trait, not modifying the core platform.
- **Performant and safe:** Rust/WASM core ensures consistent frame rates and memory safety across all browsers. No garbage collection pauses, no framework overhead, no runtime surprises.
- **Respectful of bandwidth:** Corporate networks are shared resources. The platform is designed to be a good citizen — low tick rates, compact binary serialization, and minimal bandwidth consumption even with 8 concurrent players.

---

## 3. System Architecture

### 3.1 High-Level Architecture

Breakpoint follows a host-authoritative client-server model where the host player's browser acts as both a game client and the authoritative server. All other players connect as lightweight clients. A separate, optional relay service can be deployed for NAT traversal or when the host cannot accept direct connections.

```
┌─────────────────────────────────────────────────────────────────┐
│                        HOST MACHINE                             │
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐   │
│  │  Axum Server │◄──►│  Game Engine  │◄──►│  WASM Client     │   │
│  │  (Rust)      │    │  (Authority)  │    │  (Host Player)   │   │
│  └──────┬───────┘    └──────────────┘    └──────────────────┘   │
│         │                                                       │
│         ├── WSS: Game state broadcast ──────────────────────┐   │
│         ├── REST: /api/v1/events (ingestion) ◄── Webhooks   │   │
│         ├── SSE: /api/v1/events/stream ─────────────────┐   │   │
│         └── HTTPS: Static assets (WASM, HTML, JS)       │   │   │
└─────────┼───────────────────────────────────────────────┼───┼───┘
          │                                               │   │
          ▼                                               ▼   ▼
┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐
│  Player Client   │  │  Player Client   │  │  Player Client   │
│  (WASM Browser)  │  │  (WASM Browser)  │  │  (WASM Browser)  │
│                  │  │                  │  │                  │
│  ┌────────────┐  │  │  ┌────────────┐  │  │  ┌────────────┐  │
│  │ Game View  │  │  │  │ Game View  │  │  │  │ Game View  │  │
│  ├────────────┤  │  │  ├────────────┤  │  │  ├────────────┤  │
│  │  Overlay   │  │  │  │  Overlay   │  │  │  │  Overlay   │  │
│  └────────────┘  │  │  └────────────┘  │  │  └────────────┘  │
└──────────────────┘  └──────────────────┘  └──────────────────┘

External Event Sources:
  GitHub Webhooks ──► POST /api/v1/webhooks/github
  Custom Agents  ──► POST /api/v1/events
  Polling Adapter ──► GET GitHub API ──► POST /api/v1/events
```

### 3.2 Component Overview

| Component | Technology | Responsibility |
|-----------|-----------|----------------|
| Game Engine | Rust + WASM (Bevy or custom) | Physics, rendering, game logic, input handling |
| Network Layer | WSS via web-sys / wasm-bindgen | State sync, player management, latency compensation |
| Overlay System | Rust/WASM + HTML/CSS overlay | Alert rendering, notification queue, dashboard toggle |
| Alert Bus | SSE or WSS channel | Receives agent events, routes to overlay based on priority |
| Data Integration API | REST (JSON) + webhooks | Standard interface for external systems to push events |
| Host Server | Rust (Axum) | Serves static assets, WSS endpoint, alert ingestion API |

### 3.3 Network Architecture

All network communication uses WSS (WebSocket Secure) over port 443 to ensure compatibility with corporate firewalls and proxy servers. The architecture supports two deployment modes:

**Direct Host Mode:** The hosting player runs a lightweight Rust server binary on their machine that serves the WASM game client and handles WebSocket connections. Other players connect directly to the host's URL. This is the simplest mode and works when the host has a reachable address (e.g., within a corporate VPN or LAN).

**Relay Mode:** A stateless relay server (deployable as a Cloudflare Worker, Fly.io app, or similar) handles connection brokering. The host connects to the relay as a privileged client, and all game state flows through the relay. This adds ~10–30ms latency but eliminates NAT traversal issues entirely. The relay is intentionally stateless — it forwards messages between the host and clients without understanding game logic, making it cheap to run and trivial to scale.

**Hybrid Mode (recommended for enterprise):** The host server is deployed as a persistent internal service (e.g., on a shared team server or Kubernetes pod) rather than on a developer's laptop. This provides a stable URL, avoids port-forwarding issues, and allows the alert ingestion endpoints to receive webhooks reliably. The game still runs in the browser; only the coordination layer is centralized.

### 3.4 State Synchronization Model

Since all players act simultaneously, the system uses a client-side prediction model with server reconciliation:

1. Each client runs the full game simulation locally for immediate responsiveness.
2. Clients send player inputs (not state) to the host at the game's tick rate (10–20 Hz for casual games).
3. The host runs the authoritative simulation, processes all inputs, and broadcasts the canonical game state.
4. Clients reconcile their local state with the host's authoritative state, interpolating other players' positions for smooth rendering.

For the target game types (mini-golf, platformer, laser tag), this model provides responsive local feel with authoritative fairness. The tick rate is kept deliberately low (10–20 Hz rather than 60+) because the games are casual and the reduced bandwidth is important for corporate network politeness.

### 3.5 Dual-Channel Communication

The client maintains two independent communication channels:

**Game Channel (WSS):** Carries player inputs and authoritative game state. Binary MessagePack serialization for compactness. Tick-rate-aligned — messages are sent at fixed intervals, not per-frame.

**Alert Channel (SSE or WSS):** Carries agent events from the overlay system. JSON serialization for debuggability. Event-driven — messages arrive only when events occur, not on a fixed schedule. This channel is independent of the game lifecycle; it continues operating in the lobby, between rounds, and during gameplay.

This separation ensures that a flood of agent events cannot impact game performance, and that game state synchronization doesn't delay alert delivery.

---

## 4. Game Designs

Each game is implemented as a self-contained WASM module that plugs into the shared Breakpoint runtime (networking, overlay, player management). Games share a common trait interface for lifecycle management, input handling, and state serialization.

### 4.1 Simultaneous Mini-Golf

**Genre:** Physics puzzle / racing hybrid
**Players:** 2–8
**Round Duration:** 60–90 seconds per hole
**Network Demand:** Low (10 Hz position broadcast)

All players putt simultaneously on the same course. Each player sees their own ball in full color and other players' balls as colored ghosts with trailing particle effects. The first player to sink their ball earns bonus points; remaining players earn points based on stroke count once the timer expires.

**Course Design:**
- Courses are defined as JSON data — geometry, obstacles, and spawn points — making them easy to create, share, and contribute.
- Obstacle types include static walls, bumpers (elastic bounce), windmills (rotating blockers), teleporter pads, conveyor surfaces, and moving platforms.
- A course editor (stretch goal) allows the host to build custom courses in the lobby and share them with the room.
- Course packs can be themed (e.g., "corporate campus," "retro arcade," "space station") for variety.

**Physics Model:**
- Simple 2D physics with velocity, friction, and elastic collisions against obstacles.
- Each client runs its own ball physics independently — balls don't interact with each other, so determinism between clients is not required.
- Ball positions are broadcast at 10 Hz. Other players' balls are interpolated between updates for smooth rendering.
- The host's simulation is authoritative only for scoring (hole detection and stroke counting), not for ball physics. This dramatically simplifies the networking.

**Scoring:**
- First to sink: +3 bonus points
- Under par: +2 per stroke under par
- At par: +1
- Over par: 0
- Did not finish: -1
- Running total across all holes; highest score wins the round.

**Why This Game First:** Mini-golf is the ideal starting game because it has the lowest networking requirements (balls don't interact, low tick rate), simple physics (2D collisions), and naturally supports simultaneous play. It validates the entire platform architecture with minimal game-specific complexity.

### 4.2 Platform Racer

**Genre:** Competitive 2D platformer
**Players:** 2–6
**Round Duration:** 30–60 seconds per stage
**Network Demand:** Medium (15–20 Hz position + state broadcast)

Players race through procedurally assembled obstacle courses. All players are visible on screen simultaneously. The camera follows the player's own position, with other players rendered at their world positions (potentially off-screen, shown as edge-of-screen indicators with distance).

**Game Modes:**

*Race Mode:* First to reach the end wins the round. Courses scroll horizontally with increasing difficulty. Players who fall off respawn at the nearest checkpoint with a time penalty.

*Survival Mode:* A rising hazard (lava, water, void) eliminates players from the bottom of the screen. Last one standing wins. The playfield gets increasingly cramped as the hazard rises, forcing players into tighter spaces.

**Player Interaction:**
- Collision between players is configurable per room: disabled for pure racing (ghost mode), enabled for chaotic bumping (contact mode).
- In contact mode, players can bump each other off platforms but cannot directly damage or eliminate each other — only the environment kills.
- Power-ups spawn on the course: speed boost (3s), double jump (single use), shield (absorbs one hazard hit), magnet (auto-collects nearby pickups).

**Course Generation:**
- Courses are assembled from hand-designed chunks (sections of ~10 tiles wide) that snap together procedurally.
- Chunk difficulty is tagged (easy, medium, hard) and courses ramp difficulty progressively.
- Seed-based generation ensures all players see the identical course per round.

**Technical Notes:** Requires 15–20 Hz state sync for responsive platforming feel. Client-side prediction is critical here — local player movement must feel instant, with zero perceived input lag. Other players are interpolated between server updates with smooth position lerping. The host resolves finish-order disputes authoritatively using server-side timestamps.

### 4.3 Laser Tag Arena

**Genre:** Top-down arena action
**Players:** 2–8
**Round Duration:** 3–5 minutes
**Network Demand:** Higher (20 Hz position + aim + projectile broadcast)

Players navigate a top-down arena, firing lasers to tag opponents. Tagged players are briefly stunned (1.5s freeze + visual effect) and lose a point; the player with the most tags when the timer expires wins. The aesthetic is explicitly corporate-friendly: bright neon colors, glowing particle effects, satisfying sound design, zero violence. Think laser tag facility, not shooter game.

**Arena Design:**
- Arenas feature walls for cover, open areas for confrontation, and special surfaces.
- Reflective walls bounce lasers (up to 2 bounces), enabling trick shots and indirect tags.
- Smoke zones obscure player visibility but don't block lasers.
- Power-up zones rotate on a timer: rapid fire (2x fire rate for 5s), shield (blocks next incoming tag), speed boost (1.5x move speed for 4s), wide beam (2x laser width for 3s).

**Team Mode:**
- Optional team-based play (2v2, 3v3, 4v4) with team-colored lasers that don't tag teammates.
- Team score is the sum of individual tags. Encourages coordination over individual play.

**Visual Language:**
- Players are represented as glowing circles/avatars with directional indicators showing aim.
- Lasers are bright neon lines with bloom effects. Hits produce a satisfying burst of particles.
- Stunned players pulse with a "tagged" indicator and are visually dimmed.
- No health bars, no death, no respawn timers. Being tagged is a brief inconvenience, not an elimination.

**Technical Notes:** This is the most network-intensive game due to real-time position + aim direction broadcasting. Target 20 Hz tick rate. Laser hit detection is host-authoritative to prevent cheating — clients send "fire" inputs with aim direction, the host resolves hits against authoritative player positions and broadcasts results. Laser projectiles are fast enough that client-side prediction of hits works well visually; the host confirms/denies within 1–2 frames, and mispredictions are rare enough to not feel jarring.

---

## 5. Alert Overlay System

The overlay is a transparent layer rendered on top of any active game. It operates on its own communication channel, independent of game state, so that alerts function identically regardless of which game is running (or if no game is active and players are in the lobby).

### 5.1 Alert Priority Tiers

| Tier | Behavior | Visual Treatment | Example Events |
|------|----------|-----------------|----------------|
| **Ambient** | Passive ticker, no interruption | Scrolling text bar at top or bottom edge | Agent completed task, test suite passed, PR merged automatically |
| **Notice** | Toast notification, auto-dismiss after 8s | Slide-in card from corner with summary and action button | Build failed, new issue opened, agent requests clarification |
| **Urgent** | Persistent banner until acknowledged | Top banner with pulsing border, requires click to dismiss | Production deploy awaiting approval, security alert, agent blocked |
| **Critical** | Game pause + modal overlay | Semi-transparent overlay dims game, modal with full context and actions | Production incident detected, agent attempted destructive action, compliance flag |

### 5.2 Overlay UI Elements

**Ambient Ticker:** A slim, semi-transparent bar at the top or bottom of the screen (position configurable) that scrolls agent activity updates. Players can glance at it without losing game focus. The ticker intelligently aggregates events — if 12 tests passed in the last minute, it shows "✓ 12 tests passed" rather than 12 individual messages. Events older than 5 minutes age out of the ticker automatically. The ticker also shows a subtle "heartbeat" indicator for each active agent, so the team can see at a glance that agents are running without needing specific events.

**Toast Notifications:** Slide-in cards from the bottom-right corner containing a brief summary, the source system icon (e.g., GitHub logo), a timestamp, the actor name (with an agent badge if applicable), and an optional action button (e.g., "View PR" which opens the link in a new tab). Multiple toasts stack vertically with a maximum visible count of 3; overflow is queued and displayed as toasts dismiss. Toasts can be manually dismissed with a click or swipe gesture.

**Dashboard Toggle:** A hotkey (configurable, default `Tab`) that overlays a semi-transparent dashboard showing the full agent work queue, recent activity log, and system status. The game continues rendering underneath but player input is suspended — the game is effectively paused for the player viewing the dashboard while other players continue. The dashboard shows:
- Active agent sessions with status indicators (working, waiting, blocked, idle)
- Pending action items sorted by priority
- Recent event log with filtering by source, priority, and event type
- Aggregate statistics: events per minute, agent uptime, success/failure ratios

**Claim System:** For actionable alerts (events with `action_required: true`), any player can claim it by clicking a "Handle" button on the notification. Once claimed, all other players see the alert marked as "Handled by [name]" and it auto-dismisses after 3 seconds. This prevents duplicate work and provides verbal coordination cues on the call ("I'll grab that one"). The claim is recorded in the event log for post-session review. If a claimed alert isn't resolved within a configurable timeout, it re-surfaces as unhandled.

**Alert Queue Indicator:** A small badge in the corner of the screen shows the count of unacknowledged notices and pending action items. Clicking the badge opens the dashboard directly to the pending items view. This ensures alerts are never truly lost even if a player was focused on gameplay when they arrived.

### 5.3 Sound Design

The overlay uses distinct, non-game audio cues for alerts to ensure they are perceptually separable from game sounds:

- **Ambient events:** No sound. Visual ticker only.
- **Notice events:** A soft, two-note chime (ascending). Subtle enough to register without breaking game flow.
- **Urgent events:** A three-note attention pattern (ascending, distinct from notice). Repeated once after 10 seconds if not acknowledged.
- **Critical events:** A distinctive four-note alert tone that is clearly different from all game audio. Cannot be missed.

All overlay sounds are independently volume-controlled and can be muted without affecting game audio. Sound preferences persist per player across sessions via local storage.

### 5.4 Overlay Customization

The overlay is configurable per room and per player:

**Room-level settings (host controls):**
- Which event sources are enabled
- Priority mapping overrides (e.g., elevate all deploy events to urgent)
- Ticker position (top/bottom)
- Dashboard auto-expand between rounds (on/off)
- Critical alert behavior (pause game for all players vs. only the claiming player)

**Player-level settings:**
- Sound volume for each priority tier
- Toast position (any corner)
- Dashboard hotkey binding
- Notification density preference (show all / compact / critical only)

---

## 6. Data Integration Interface

The data integration interface is the public contract that external systems use to feed events into Breakpoint. It is designed to be vendor-agnostic, simple to implement, and comprehensive enough to represent the full spectrum of agent activity. The reference implementation provides adapters for GitHub, but the interface itself makes no assumptions about the source system.

### 6.1 Event Schema

All events are JSON objects conforming to the following base schema. Events are posted to the Breakpoint host's ingestion endpoint via HTTP POST.

```json
{
  "event_type": "pipeline.failed",
  "source": "github",
  "priority": "notice",
  "title": "CI failed on main: test_integration",
  "body": "3 tests failed in test_integration suite. Failures: test_user_auth, test_payment_flow, test_rate_limit. See workflow run for details.",
  "timestamp": "2026-02-12T14:32:00Z",
  "url": "https://github.com/org/repo/actions/runs/12345",
  "actor": "github-actions[bot]",
  "tags": ["repo:breakpoint", "branch:main", "workflow:ci"],
  "action_required": false,
  "metadata": {
    "workflow_name": "CI",
    "run_number": 847,
    "failure_count": 3,
    "duration_seconds": 142
  }
}
```

**Field Reference:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `event_type` | string (enum) | Yes | Category of the event (see Event Types below) |
| `source` | string | Yes | Identifier for the originating system (e.g., "github", "jenkins", "custom-agent") |
| `priority` | string (enum) | No | `ambient` \| `notice` \| `urgent` \| `critical` (defaults to `ambient`) |
| `title` | string | Yes | Short summary displayed in notifications (max 120 chars) |
| `body` | string | No | Extended detail shown in dashboard view or modal (max 2000 chars, supports markdown) |
| `timestamp` | ISO 8601 | Yes | When the event occurred in the source system |
| `url` | string (URL) | No | Deep link to the relevant resource (opens in new tab on click) |
| `actor` | string | No | Name or identifier of the agent or user that triggered the event |
| `tags` | string[] | No | Arbitrary labels for filtering (e.g., `["repo:breakpoint", "env:prod"]`) |
| `action_required` | boolean | No | If `true`, the alert is claimable and persists until acknowledged |
| `group_key` | string | No | Events with the same group_key are aggregated in the ticker (e.g., "test-results-run-847") |
| `expires_at` | ISO 8601 | No | Auto-dismiss the alert after this time (useful for time-sensitive approvals) |
| `metadata` | object | No | Arbitrary key-value data for custom rendering or filtering |

### 6.2 Event Types

The following event types are recognized by the default overlay renderer. Unknown event types are treated as generic notifications with an "info" icon.

| Event Type | Description | Default Priority |
|------------|-------------|-----------------|
| `pipeline.started` | CI/CD pipeline or workflow began | ambient |
| `pipeline.succeeded` | Pipeline completed successfully | ambient |
| `pipeline.failed` | Pipeline failed | notice |
| `pr.opened` | Pull request opened by agent or human | notice |
| `pr.reviewed` | Review submitted on a PR | ambient |
| `pr.merged` | PR merged into target branch | ambient |
| `pr.conflict` | Merge conflict requires resolution | notice |
| `issue.opened` | New issue created | ambient |
| `issue.assigned` | Issue assigned to team member or agent | ambient |
| `issue.closed` | Issue closed | ambient |
| `review.requested` | Agent requests human review | notice |
| `deploy.pending` | Deployment awaiting human approval | urgent |
| `deploy.completed` | Deployment finished | ambient |
| `deploy.failed` | Deployment failed | urgent |
| `agent.started` | Agent began a task | ambient |
| `agent.completed` | Agent finished a task | ambient |
| `agent.blocked` | Agent cannot proceed without human input | urgent |
| `agent.error` | Agent encountered an unrecoverable error | notice |
| `security.alert` | Security vulnerability or policy violation | critical |
| `comment.added` | Comment posted on PR or issue | ambient |
| `branch.pushed` | Commits pushed to a branch | ambient |
| `test.passed` | Test suite passed (aggregatable) | ambient |
| `test.failed` | Test suite failed | notice |
| `custom` | Generic custom event | ambient |

### 6.3 Integration Endpoints

#### 6.3.1 Push: Event Ingestion

**`POST /api/v1/events`**

Accepts a single event or a batch of events (JSON array). Authentication is via a shared secret token passed in the `Authorization: Bearer <token>` header. Rate-limited to 100 events per minute per source to prevent overlay flooding.

```bash
# Single event
curl -X POST https://breakpoint.internal:8443/api/v1/events \
  -H "Authorization: Bearer abc123" \
  -H "Content-Type: application/json" \
  -d '{"event_type":"pipeline.failed","source":"github","title":"CI failed on main","timestamp":"2026-02-12T14:32:00Z"}'

# Batch events
curl -X POST https://breakpoint.internal:8443/api/v1/events \
  -H "Authorization: Bearer abc123" \
  -H "Content-Type: application/json" \
  -d '[{"event_type":"test.passed","source":"github","title":"Unit tests passed","timestamp":"..."},{"event_type":"test.failed","source":"github","title":"Integration tests failed","timestamp":"..."}]'
```

#### 6.3.2 Push: Webhook Receiver

**`POST /api/v1/webhooks/{source}`**

Accepts native webhook payloads from supported platforms and transforms them into Breakpoint events using built-in adapter logic. The `{source}` path parameter selects the appropriate adapter. Signature verification is performed per platform (e.g., GitHub's `X-Hub-Signature-256` HMAC verification).

```bash
# GitHub webhook (configured in repo settings)
# URL: https://breakpoint.internal:8443/api/v1/webhooks/github
# Content type: application/json
# Secret: <shared webhook secret>
# Events: workflow_run, pull_request, push, issues, issue_comment, deployment_status
```

#### 6.3.3 Pull: Status Query

**`GET /api/v1/status`**

Returns the current state of all active event sources, pending action items, and recent event history. Powers the dashboard overlay and can be polled by external monitoring tools.

```json
{
  "active_sources": ["github", "custom-agent-1"],
  "pending_actions": [
    {
      "event_id": "evt_abc123",
      "title": "Deploy to prod awaiting approval",
      "priority": "urgent",
      "claimed_by": null,
      "created_at": "2026-02-12T14:30:00Z"
    }
  ],
  "recent_events": [ ... ],
  "stats": {
    "events_last_hour": 47,
    "events_last_minute": 3,
    "agents_active": 2,
    "agents_blocked": 0
  }
}
```

#### 6.3.4 Stream: Server-Sent Events

**`GET /api/v1/events/stream`**

An SSE endpoint that streams events in real-time. Used by the overlay system internally but also available for external consumers (e.g., a separate dashboard application, mobile notifications, or logging infrastructure).

```
GET /api/v1/events/stream
Accept: text/event-stream
Authorization: Bearer abc123

data: {"event_type":"pipeline.started","source":"github","title":"CI started on feature/auth",...}

data: {"event_type":"pr.opened","source":"github","title":"Agent opened PR #142: Add rate limiting",...}
```

#### 6.3.5 Event Acknowledgment

**`POST /api/v1/events/{event_id}/claim`**

Claims an actionable event. The claiming player's display name is broadcast to all connected clients.

```json
{
  "claimed_by": "Andrew",
  "claimed_at": "2026-02-12T14:35:00Z"
}
```

---

## 7. Reference Implementation: GitHub Integration

The reference integration demonstrates monitoring a complete GitHub-based development workflow. It covers the most common agent activities in a GitOps-driven team and serves as both a usable integration and a comprehensive example for building custom adapters.

### 7.1 GitHub Webhook Adapter

The GitHub adapter registers for the following webhook events and transforms them into Breakpoint event schema:

| GitHub Webhook | Breakpoint Event Type | Mapping Notes |
|---------------|-------------------|---------------|
| `workflow_run` | `pipeline.*` | Maps to `started`/`succeeded`/`failed` based on `action` + `conclusion` |
| `pull_request` | `pr.*` | Maps `opened`/`closed`/`merged`/`review_requested` actions |
| `pull_request_review` | `pr.reviewed` | Includes review state (approved/changes_requested/commented) |
| `issues` | `issue.*` | Maps `opened`/`assigned`/`closed` actions |
| `issue_comment` | `comment.added` | Includes context (PR vs issue, author is bot or human) |
| `push` | `branch.pushed` | Includes commit count, branch name, comparison URL |
| `deployment_status` | `deploy.*` | Maps `pending`/`success`/`failure`/`error` states |
| `check_suite` | `pipeline.*` | Alternative to `workflow_run` for check-based CI systems |
| `dependabot_alert` | `security.alert` | Auto-elevates to `critical` priority for high/critical severity |

### 7.2 GitHub Actions Monitor

Beyond webhooks, the GitHub adapter includes a polling component that queries the GitHub Actions API every 30 seconds to provide richer pipeline monitoring. This catches events that webhooks may miss (e.g., manual re-runs, queued workflows) and provides aggregate statistics:

- Active workflow runs with real-time step progress (which step is currently executing)
- Queue depth and estimated wait times for self-hosted runners
- Historical success/failure rates per workflow (rolling 24-hour window)
- Runner utilization and billing minute consumption
- Stale workflow detection (runs exceeding expected duration thresholds)

This data feeds both the ambient ticker (aggregate stats like "CI: 94% pass rate today") and the dashboard overlay (detailed per-workflow view with step-level progress).

### 7.3 Agent Activity Inference

The adapter infers agent vs. human activity by examining commit authors, PR creators, and comment authors against a configurable list of known bot/agent accounts. Default detection patterns include:

- GitHub's built-in bots: `dependabot[bot]`, `github-actions[bot]`, `renovate[bot]`
- Common AI coding agents: configurable patterns like `*[bot]`, `*-agent`, or explicit username lists
- Custom agent identifiers specified in the Breakpoint configuration

Agent-originated events are visually distinguished in the overlay with a robot icon badge and a different accent color, allowing the team to quickly differentiate between human team activity and autonomous agent activity. The dashboard provides filtered views: "All Events," "Agent Only," and "Human Only."

### 7.4 Example: Complete GitHub Setup

```yaml
# .github/workflows/breakpoint-notify.yml
# Optional: Explicit notification workflow for custom events
name: Breakpoint Notify
on:
  workflow_run:
    workflows: ["CI", "Deploy"]
    types: [completed]

jobs:
  notify:
    runs-on: ubuntu-latest
    steps:
      - name: Notify Breakpoint
        run: |
          STATUS=${{ github.event.workflow_run.conclusion }}
          EVENT_TYPE="pipeline.succeeded"
          PRIORITY="ambient"
          if [ "$STATUS" = "failure" ]; then
            EVENT_TYPE="pipeline.failed"
            PRIORITY="notice"
          fi
          curl -X POST ${{ secrets.BREAKPOINT_URL }}/api/v1/events \
            -H "Authorization: Bearer ${{ secrets.BREAKPOINT_TOKEN }}" \
            -H "Content-Type: application/json" \
            -d "{
              \"event_type\": \"$EVENT_TYPE\",
              \"source\": \"github\",
              \"priority\": \"$PRIORITY\",
              \"title\": \"${{ github.event.workflow_run.name }}: $STATUS\",
              \"url\": \"${{ github.event.workflow_run.html_url }}\",
              \"actor\": \"${{ github.event.workflow_run.actor.login }}\",
              \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
              \"tags\": [\"repo:${{ github.repository }}\", \"branch:${{ github.event.workflow_run.head_branch }}\"]
            }"
```

---

## 8. Lobby and Room System

### 8.1 Room Creation and Joining

When a host starts Breakpoint, they create a room that generates a shareable URL containing a room code (e.g., `breakpoint.example.com/room/ABCD-1234`). The room code is short enough to read aloud on a call. Players joining via the URL enter a lobby where they can:

- Set their display name (persisted in local storage for future sessions)
- Choose an avatar color from a preset palette
- See other connected players and their readiness status
- View the overlay feed (alerts are active even before a game starts)

The host has additional controls in the lobby: game selection, room settings (max players, round count, timer duration), overlay configuration, and a "Start Game" button.

### 8.2 Between-Round Lobby

Between game rounds, players return to a mini-lobby showing:

- The current scoreboard with round-by-round breakdown
- Next-round countdown timer (configurable 15–60 seconds, default 30s)
- A text chat panel for quick messages (supplements voice on the call)
- The dashboard overlay automatically expanded during the between-round period

This auto-expansion is a key design choice. It creates an organic rhythm of play → review → play that keeps both the social and supervisory functions active. The between-round window is the natural time for the team to discuss agent activity, handle pending actions, and verbally coordinate before the next round begins.

### 8.3 Late Join and Spectator Mode

Players who join mid-game enter spectator mode and automatically become active players at the start of the next round. Spectators can:

- Watch the current game in real-time
- See the full overlay and dashboard
- Claim and handle alerts (they're still part of the team, just not playing yet)
- Chat in the text panel

This makes spectator mode useful beyond late-joining — team leads or managers who want to monitor agent activity during office hours without actively playing can join as permanent spectators.

### 8.4 Room Persistence

Rooms persist as long as the host is connected. If the host disconnects, a configurable grace period (default 60 seconds) allows reconnection before the room closes. Optionally, host migration can promote the longest-connected player to host if the original host leaves permanently.

---

## 9. Technology Stack

| Layer | Technology | Rationale |
|-------|-----------|-----------|
| Core Language | Rust | Memory safety, performance, single codebase for server + WASM client |
| Client Runtime | WASM (wasm-bindgen, web-sys) | Near-native browser performance, no plugins, broad browser support |
| Game Framework | Bevy (or custom ECS) | Bevy has WASM support and active ecosystem; fallback to custom for lighter WASM footprint |
| Rendering | wgpu via Bevy / Canvas2D fallback | WebGPU where available, Canvas2D as universal fallback for corporate browsers |
| Networking | WSS (tokio-tungstenite server, web-sys client) | Firewall-friendly, bidirectional, low overhead |
| Host Server | Axum | Lightweight, async, Rust-native, excellent WebSocket support |
| Alert Ingestion | Axum REST endpoints + SSE | Standard HTTP for webhook compatibility, SSE for real-time streaming |
| Serialization | serde + MessagePack (rmp-serde) | JSON for API surface, MessagePack for game state (compact binary) |
| Audio | Web Audio API via web-sys | Low-latency game and notification audio |
| Build | cargo + wasm-pack + trunk | Trunk for dev server with hot reload; wasm-pack for optimized production WASM |
| CI/CD | GitHub Actions | Automated build, test, WASM compilation, and release packaging |
| Testing | cargo test + wasm-bindgen-test | Native tests for game logic, WASM tests for browser integration |

### 9.1 Bevy vs. Custom Engine Decision

Bevy is the default choice due to its ECS architecture, WASM support, and growing ecosystem. However, Bevy's WASM bundle size (~5–10MB) may be problematic for fast initial loads on corporate networks. If bundle size becomes an issue, a lighter custom engine using `web-sys` Canvas2D directly is the fallback. The game trait interface (Section 11) is designed to be engine-agnostic, so games can be ported between Bevy and a custom renderer without rewriting game logic.

The decision point is during Phase 1: if the Bevy WASM build exceeds 8MB gzipped, switch to custom Canvas2D rendering. 2D games with simple geometry do not need a full ECS engine.

---

## 10. Project Structure

```
breakpoint/
├── Cargo.toml                          # Workspace root
├── README.md
├── LICENSE-MIT
├── LICENSE-APACHE
├── crates/
│   ├── breakpoint-core/                   # Shared types, traits, event schema
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── events.rs               # Event schema, priority types, serialization
│   │       ├── game_trait.rs            # BreakpointGame trait definition
│   │       ├── player.rs               # Player identity, state, avatar
│   │       ├── room.rs                 # Room configuration, lifecycle
│   │       ├── net/
│   │       │   ├── mod.rs
│   │       │   ├── messages.rs         # Network message types (input, state, alert)
│   │       │   └── protocol.rs         # Versioned protocol definition
│   │       └── overlay/
│   │           ├── mod.rs
│   │           ├── ticker.rs           # Ambient ticker aggregation logic
│   │           ├── toast.rs            # Toast notification queue
│   │           └── dashboard.rs        # Dashboard data model
│   │
│   ├── breakpoint-server/                 # Axum host server
│   │   └── src/
│   │       ├── main.rs                 # Server entry point, configuration
│   │       ├── ws.rs                   # WebSocket game state relay
│   │       ├── api.rs                  # REST event ingestion endpoints
│   │       ├── sse.rs                  # SSE event stream
│   │       ├── room_manager.rs         # Room lifecycle, player tracking
│   │       ├── auth.rs                 # Token verification, webhook signatures
│   │       └── webhooks/               # Webhook adapter modules
│   │           ├── mod.rs
│   │           ├── github.rs           # GitHub webhook → Breakpoint event
│   │           └── generic.rs          # Passthrough for pre-formatted events
│   │
│   ├── breakpoint-client/                 # WASM browser client
│   │   └── src/
│   │       ├── lib.rs                  # WASM entry point
│   │       ├── app.rs                  # Application state machine (lobby/game/between)
│   │       ├── lobby.rs                # Room join/create UI
│   │       ├── renderer.rs             # Canvas2D / wgpu rendering abstraction
│   │       ├── input.rs                # Keyboard/mouse/touch input handling
│   │       ├── net_client.rs           # WSS client, message send/receive
│   │       ├── overlay.rs              # Alert overlay rendering
│   │       ├── dashboard.rs            # Full dashboard toggle view
│   │       ├── audio.rs                # Sound manager (game + overlay)
│   │       └── storage.rs              # Local storage for preferences
│   │
│   ├── games/
│   │   ├── breakpoint-golf/               # Mini-golf game module
│   │   │   └── src/
│   │   │       ├── lib.rs              # BreakpointGame trait implementation
│   │   │       ├── physics.rs          # Ball physics, collision
│   │   │       ├── course.rs           # Course data, obstacles, generation
│   │   │       ├── renderer.rs         # Golf-specific rendering
│   │   │       └── scoring.rs          # Stroke counting, par, bonuses
│   │   │
│   │   ├── breakpoint-platformer/         # Platform racer module
│   │   │   └── src/
│   │   │       ├── lib.rs
│   │   │       ├── physics.rs          # Platformer physics, gravity, collision
│   │   │       ├── course_gen.rs       # Procedural course assembly
│   │   │       ├── renderer.rs
│   │   │       └── powerups.rs
│   │   │
│   │   └── breakpoint-lasertag/           # Laser tag module
│   │       └── src/
│   │           ├── lib.rs
│   │           ├── arena.rs            # Arena layout, special surfaces
│   │           ├── projectile.rs       # Laser physics, reflection
│   │           ├── renderer.rs
│   │           └── powerups.rs
│   │
│   └── adapters/
│       └── breakpoint-github/             # GitHub polling + webhook adapter
│           └── src/
│               ├── lib.rs
│               ├── poller.rs           # GitHub Actions API polling
│               ├── transformer.rs      # Webhook payload → Breakpoint event
│               └── agent_detect.rs     # Bot/agent identification logic
│
├── web/                                # Static assets
│   ├── index.html                      # HTML shell for WASM client
│   ├── style.css                       # Minimal base styles
│   └── assets/
│       ├── sounds/                     # Audio files for game + overlay
│       └── sprites/                    # Game sprites and UI elements
│
├── docs/
│   ├── ARCHITECTURE.md
│   ├── INTEGRATION-GUIDE.md            # How to build custom adapters
│   ├── GAME-DEVELOPMENT.md             # How to add new games
│   └── DEPLOYMENT.md                   # Corporate deployment guide
│
└── examples/
    ├── github-actions-monitor/         # Complete GitHub Actions setup
    │   ├── README.md
    │   └── .github/workflows/breakpoint-notify.yml
    ├── custom-agent-adapter/           # Example custom agent integration
    │   ├── README.md
    │   └── notify.py                   # Simple Python script example
    └── docker-compose.yml              # One-command local deployment
```

---

## 11. Game Trait Interface

All games implement a common trait that the Breakpoint runtime uses for lifecycle management. This is the contract that makes adding new games possible without modifying the core platform.

```rust
/// Core trait that all Breakpoint games must implement.
/// The runtime manages networking, overlay, and player tracking;
/// the game only handles game-specific logic and rendering.
pub trait BreakpointGame {
    /// Game metadata for the lobby selection screen.
    fn metadata(&self) -> GameMetadata;

    /// Called once when the game is selected and players are ready.
    /// Receives the player list and room configuration.
    fn init(&mut self, players: &[Player], config: &GameConfig);

    /// Called each frame. Delta time in seconds.
    /// Returns a list of game events (scoring, elimination, round end).
    fn update(&mut self, dt: f32, inputs: &PlayerInputs) -> Vec<GameEvent>;

    /// Render the current game state to the provided render context.
    fn render(&self, ctx: &mut RenderContext);

    /// Serialize the authoritative game state for network broadcast.
    /// Called on the host at the tick rate.
    fn serialize_state(&self) -> Vec<u8>;

    /// Apply authoritative state received from the host.
    /// Called on clients when a state update arrives.
    fn apply_state(&mut self, state: &[u8]);

    /// Serialize local player input for sending to the host.
    fn serialize_input(&self, player_id: PlayerId) -> Vec<u8>;

    /// Apply a remote player's input to the authoritative simulation.
    /// Called on the host when input arrives from a client.
    fn apply_input(&mut self, player_id: PlayerId, input: &[u8]);

    /// Called when a new player joins mid-game (for spectator → active transition).
    fn player_joined(&mut self, player: &Player);

    /// Called when a player disconnects.
    fn player_left(&mut self, player_id: PlayerId);

    /// Whether the game supports the overlay pausing gameplay
    /// (e.g., for critical alerts). Games can return false if
    /// pausing would break game state (rare).
    fn supports_pause(&self) -> bool { true }

    /// Called when the overlay requests a pause (critical alert).
    fn pause(&mut self);

    /// Called when gameplay should resume after a pause.
    fn resume(&mut self);

    /// Whether the current round/match is complete.
    fn is_round_complete(&self) -> bool;

    /// Final scores for the completed round.
    fn round_results(&self) -> Vec<PlayerScore>;
}

pub struct GameMetadata {
    pub name: String,
    pub description: String,
    pub min_players: u8,
    pub max_players: u8,
    pub estimated_round_duration: Duration,
    pub icon: GameIcon,
}

pub struct GameConfig {
    pub round_count: u8,
    pub round_duration: Duration,
    pub custom: HashMap<String, serde_json::Value>,  // Game-specific settings
}
```

This trait is designed so that a new game can be developed in isolation, tested locally, and plugged into the Breakpoint runtime with zero changes to the networking, overlay, lobby, or server code. The `custom` field in `GameConfig` allows each game to expose its own settings (e.g., course length for golf, collision mode for platformer, team size for laser tag) without polluting the core configuration.

---

## 12. Network Protocol

### 12.1 Message Types

All messages are MessagePack-serialized with a 1-byte type discriminator prefix.

```rust
#[repr(u8)]
pub enum MessageType {
    // Client → Host
    PlayerInput     = 0x01,  // Per-tick player input
    JoinRoom        = 0x02,  // Initial room join request
    LeaveRoom       = 0x03,  // Graceful disconnect
    ClaimAlert      = 0x04,  // Claim an actionable alert
    ChatMessage     = 0x05,  // Text chat

    // Host → Client
    GameState       = 0x10,  // Authoritative game state snapshot
    PlayerList      = 0x11,  // Updated player list (join/leave)
    RoomConfig      = 0x12,  // Room settings (game selection, config)
    GameStart       = 0x13,  // Transition from lobby to game
    RoundEnd        = 0x14,  // Round results, return to between-round lobby
    GameEnd         = 0x15,  // Final results, return to main lobby

    // Host → Client (Alert channel, may also use SSE)
    AlertEvent      = 0x20,  // New alert event
    AlertClaimed    = 0x21,  // Alert claimed by a player
    AlertDismissed  = 0x22,  // Alert auto-dismissed or expired
}
```

### 12.2 Bandwidth Estimation

For a typical 8-player mini-golf session:

| Data | Size | Frequency | Bandwidth |
|------|------|-----------|-----------|
| Player input (per player) | ~20 bytes | 10 Hz | 1.6 KB/s total |
| Game state broadcast | ~200 bytes | 10 Hz | 2.0 KB/s |
| Alert events | ~500 bytes | ~0.1 Hz avg | 0.05 KB/s |
| **Total** | | | **~4 KB/s** |

This is negligible on any network. Even the most network-intensive game (laser tag at 20 Hz with 8 players) stays under 15 KB/s total — well within "invisible" range for corporate networks.

---

## 13. Development Roadmap

### Phase 1: Foundation (Weeks 1–4)

- Cargo workspace setup with core, server, and client crates
- Axum server with static file serving, WSS endpoint, and room management
- WASM client shell with lobby UI (create room, join room, set display name)
- Basic state synchronization: player positions broadcast at 10 Hz
- Simultaneous mini-golf with simple 2D physics (one hardcoded course)
- Deploy and validate within a corporate network environment (VPN, proxy)

**Milestone:** Two players can join a room and play mini-golf simultaneously over WSS.

### Phase 2: Overlay System (Weeks 5–8)

- Event schema definition and REST ingestion endpoint (`POST /api/v1/events`)
- SSE event stream implementation
- Overlay rendering: ambient ticker, toast notifications, dashboard toggle
- GitHub webhook adapter (PR, push, workflow_run, issues, deployment_status)
- GitHub Actions polling monitor with aggregate statistics
- Alert claim system with multi-player acknowledgment broadcast

**Milestone:** Agent events from a real GitHub repository appear as overlay notifications during gameplay.

### Phase 3: Game Expansion (Weeks 9–12)

- Platform racer with procedural course generation and two game modes
- Laser tag arena with power-ups, reflective surfaces, and team mode
- Between-round lobby with scoreboard and dashboard auto-expand
- Spectator mode and late-join support
- Audio system: game sounds + overlay notification sounds
- Course/arena editor for host customization (stretch)

**Milestone:** Three fully playable games with seamless transitions and overlay integration.

### Phase 4: Polish and Release (Weeks 13–16)

- Custom adapter documentation and example implementations (Python, Node.js, shell script)
- Configuration UI for alert priority mapping and overlay preferences
- Performance optimization: WASM size reduction, tick rate auto-tuning, lazy asset loading
- Relay server deployment option (Fly.io / Cloudflare Worker)
- Docker Compose one-command deployment
- Open-source release: README, contributing guide, license, CI badges, release binaries
- Dogfood in production office hours

**Milestone:** Public repository with documentation, examples, release artifacts, and at least one production deployment.

---

## 14. Security Considerations

**Room Authentication:** Rooms are protected by a host-generated shared secret embedded in the URL. The URL contains the room code; the secret is part of the URL fragment (not sent to servers in HTTP referrer headers). This prevents unauthorized players from joining without requiring a full authentication system. For enterprise deployments, rooms can optionally require SSO authentication via the corporate identity provider.

**Event Ingestion Authentication:** The `/api/v1/events` endpoint requires a Bearer token. Webhook endpoints use platform-specific signature verification (e.g., GitHub's `X-Hub-Signature-256` HMAC-SHA256 header). Tokens are configurable per source, allowing different secrets for different integrations.

**Data Sensitivity:** Breakpoint is designed for developer workflow metadata, not sensitive data. Event titles and bodies should not contain secrets, credentials, or PII. The documentation will include explicit guidance on what to include and exclude from event payloads, with examples of safe vs. unsafe event content. The overlay never renders raw webhook payloads — only the transformed Breakpoint event fields are displayed.

**Transport Security:** All communication uses TLS (HTTPS/WSS). No plaintext fallback is supported. The host server can use self-signed certificates for internal deployment or trusted certificates for internet-facing deployment.

**WASM Sandbox:** The game client runs in the browser's WASM sandbox with no access to the local filesystem, system APIs, or other tabs. The only external communication is to the Breakpoint host server via the established WSS and SSE connections.

**Corporate Deployment:** For enterprise deployments behind corporate proxies, the host server can be deployed as an internal service with standard corporate certificate management. No external internet access is required for gameplay; only the webhook/polling adapters need outbound access to their respective platforms (e.g., GitHub API). These can run on a separate machine or container from the game server if network segmentation requires it.

**Rate Limiting:** The event ingestion API is rate-limited per source token (100 events/minute default, configurable). WebSocket connections are rate-limited per room (max 8 players default, configurable). These limits prevent abuse and ensure the overlay remains usable even if an external system malfunctions and floods events.

---

## 15. Open Source Strategy

**License:** MIT or Apache 2.0 dual license, standard for the Rust ecosystem. Permissive licensing encourages adoption and contribution from both individual developers and enterprises without legal friction.

**Repository:** Public GitHub repository with CI/CD that builds and tests on every push. Releases include pre-built server binaries (Linux x86_64, Linux ARM64, macOS, Windows) and the WASM client bundle as a downloadable artifact. A Docker image is published to GitHub Container Registry for one-command deployment.

**Documentation:** The reference GitHub integration serves as the primary example. Additional examples are provided for common platforms and use cases:
- GitLab CI/CD webhook adapter
- Jenkins pipeline notifications
- Custom HTTP agent (Python script example)
- Shell script one-liners for ad-hoc notifications
- The event schema documentation is thorough enough that any developer can build a custom adapter in under an hour.

**Community:** Contributions are welcome for new game modules, integration adapters, overlay themes, and course/arena designs. The game trait interface is designed to make adding new games straightforward without needing to modify the core networking or overlay systems. A `CONTRIBUTING.md` guide covers the development setup, testing approach, and PR process.

**Dogfooding:** The project will be actively used internally at Cigna to validate the concept in a real enterprise environment. Learnings from corporate deployment (firewall issues, proxy compatibility, team adoption patterns, overlay usefulness metrics) will be documented publicly to help other enterprises adopt the tool.

**Naming:** "Breakpoint" carries a triple meaning: the debugger pause where execution halts for inspection (exactly what humans do while agents run), the moment you take a break (the social/gaming layer), and the critical threshold where attention is needed (the alert overlay). The name is developer-native, immediately understood, and memorable.

---

## 16. Success Metrics

**Team Presence:** Average time spent in shared Breakpoint sessions during office hours vs. solo monitoring. Target: 70%+ of office hour duration spent in a shared session.

**Alert Response Time:** Time from agent alert to human acknowledgment, compared to baseline (Slack/email notifications). Target: 50% reduction in median response time for urgent and critical alerts.

**Duplicate Work Reduction:** Frequency of multiple engineers responding to the same alert, measured via the claim system. Target: <5% duplicate claim rate.

**Agent Utilization:** Percentage of `agent.blocked` events resolved within the office hour window vs. deferred to async. Target: 90%+ resolution within session.

**Developer Satisfaction:** Qualitative feedback on whether office hours feel more productive, connected, and sustainable with Breakpoint. Measured via periodic team surveys.

**Open Source Adoption:** GitHub stars, forks, community-contributed game modules and adapters. Number of organizations reporting production use (tracked via opt-in telemetry or community reports).

---

## 17. Future Considerations

**Additional Game Ideas:** The game trait interface supports unlimited expansion. Future games could include: trivia battles (with custom question packs per team), a drawing game (Drawful-style), territory conquest (hex grid strategy), cooperative tower defense, and typing speed races. Community-contributed games are a key growth vector.

**Mobile Support:** The WASM client runs in mobile browsers, but touch input and small-screen layouts need dedicated attention. A future phase could optimize the lobby and overlay for mobile, allowing team members to spectate and handle alerts from their phone while away from their desk.

**Voice Integration:** If the team is already on a voice call (Teams, Zoom, Discord), Breakpoint could optionally capture voice activity indicators and display them as player status in the game (speaking/muted badges). This requires no audio processing — just the WebRTC voice activity detection flag.

**Persistent Statistics:** An optional statistics backend could track per-player game performance, alert response patterns, and team metrics over time. This data could power "season" leaderboards, team retrospectives, and agent supervision effectiveness reports.

**Plugin System:** Beyond the game trait, a broader plugin system could allow custom overlay widgets (e.g., a stock ticker, weather widget, team calendar), custom alert renderers, and integration with non-development systems (e.g., support ticket queues, infrastructure monitoring).

**AI-Generated Courses:** Using generative models to create mini-golf courses, platformer levels, and laser tag arenas based on prompts. "Generate a course themed around our sprint goals" would be a fun team-building feature.

---

*This document is a living specification. It will be updated as development progresses and as learnings from dogfooding inform design decisions.*
