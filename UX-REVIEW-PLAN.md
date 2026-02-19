# Breakpoint UX Review & Improvement Plan

## Audit Summary

A comprehensive UX audit identified **5 critical gap areas** across the frontend:

1. **Dead audio integration** — 7 game sound events defined but never emitted from game code
2. **No WASM loading UX** — 1.7MB binary loads with zero progress indication; failures show blank page
3. **Missing in-game feedback** — no screen shake, no hit flash, no celebration effects, no kill feed
4. **Incomplete disconnect/error UX** — generic "Reconnecting..." with no attempt count, countdown, or recovery actions
5. **Missing UI polish** — no copy-to-clipboard for room codes, no loading spinners, no noscript fallback, no game descriptions, buttons clickable but non-functional before WASM loads

The rendering foundation is solid (5 shader programs, proper fog, DPR handling, WebGL context restore), but the layer between game logic and user perception has significant gaps.

---

## Phase 1: Critical Path — Loading, Errors & Audio Wiring

**Goal:** Users should never see a blank/broken page, and game events should have sound.

### 1A. WASM Loading UX (`web/init.js`, `web/index.html`, `web/style.css`)
- Add a loading overlay with spinner + "Loading game engine..." text, visible by default
- Wrap `await init()` in try-catch with user-facing error message on failure
- Add `<noscript>` tag telling users JavaScript is required
- Hide loading overlay once WASM `start()` completes (bridge callback)
- Disable lobby buttons until WASM is ready (CSS `:disabled` styling)

### 1B. WebGL2 Failure Handling (`app.rs`, `bridge.rs`, `web/ui.js`)
- On `Renderer::new()` failure, push error to JS layer via bridge (not just console.error)
- Show user-facing message: "Your browser doesn't support WebGL2. Please use Chrome, Firefox, or Edge."
- Add `bridge::show_fatal_error(msg)` function that displays a styled error overlay

### 1C. Wire Up Game Audio Events
- **Golf** (`golf_input.rs` or `app.rs` golf handling): emit `GolfStroke` on stroke, `GolfBallSink` when ball sinks
- **Platformer** (`platformer_input.rs` or `app.rs`): emit `PlatformerJump` on jump, `PlatformerPowerUp` on pickup, `PlatformerFinish` on finish
- **LaserTag** (`lasertag_input.rs` or `app.rs`): emit `LaserFire` on fire, `LaserHit` on being hit
- **Tron** (`audio.rs` + `app.rs`): add `TronCrash`, `TronGrind`, `TronWin` events; emit on death, grind state, win
- Remove `#[allow(dead_code)]` from `AudioEvent` enum once wired

### 1D. Disconnect Banner Improvements (`bridge.rs`, `app.rs`, `web/ui.js`)
- Pass reconnect attempt number and next-retry countdown to JS via `_breakpointDisconnect(attempt, nextRetryMs)`
- Update disconnect banner: "Connection lost. Reconnecting (attempt 3/10, retrying in 8s...)"
- Show "Connection failed. Click to return to lobby." after max retries exhausted

**Estimated scope:** ~300 lines changed across 8-10 files.

---

## Phase 2: In-Game Feedback & Visual Polish

**Goal:** Game events feel impactful with visual and audio feedback.

### 2A. Screen Shake Integration (`camera_gl.rs`, game render files)
- `camera_gl.rs` already has `apply_shake()` — wire it to game events:
  - Golf: shake on stroke
  - Platformer: shake on landing after jump, shake on elimination
  - LaserTag: shake on being hit
  - Tron: shake on crash (for spectators viewing the crash)
- Add shake intensity parameter (light=0.1, medium=0.3, heavy=0.5)

### 2B. Visual State Feedback (game render files)
- **Golf**: flash the ball briefly white on stroke; pulse the hole when ball sinks
- **Platformer**: brief glow on player when collecting power-up; dim+shrink eliminated player before removal
- **LaserTag**: brief white flash on hit player; pulsing shield glow when shield active
- **Tron**: brief explosion glow on crash location (glow shader, expanding then fading); wall color flash on grind

### 2C. Copy Room Code Button (`web/index.html`, `web/ui.js`, `web/style.css`)
- Add a "Copy" button next to room code display
- Use `navigator.clipboard.writeText()` with fallback
- Brief "Copied!" feedback text or button color change

### 2D. Game Descriptions in Lobby (`web/index.html`, `web/style.css`, `web/ui.js`)
- Add short descriptions below game selector buttons (shown on hover/selection)
  - Mini Golf: "2-8 players · Turn-based · 10 courses"
  - Platform Racer: "2-6 players · Race or Survive"
  - Laser Tag: "2-8 players · FFA or Teams"
  - Tron: "2-8 players · Light Cycles · Bots available"

**Estimated scope:** ~250 lines changed across 8-10 files.

---

## Phase 3: HUD Completeness & Overlay Polish

**Goal:** Players have all critical information during gameplay; alerts are manageable.

### 3A. Toast Auto-Dismiss & Priority (`web/ui.js`, `web/style.css`)
- Add auto-dismiss timer (8s default, configurable via theme `toast_duration`)
- Sort displayed toasts by priority (Critical > Urgent > Notice > Ambient)
- Cap simultaneous visible toasts at 5; show "+N more" indicator
- Add fade-out animation on dismiss (CSS transition)

### 3B. HUD Enhancements Per Game
- **Golf**: Add power meter visualization (bar below aim indicator); add distance-to-hole text
- **Platformer**: Add checkpoint progress indicator (e.g., "Checkpoint 2/5")
- **LaserTag**: Add kill feed (last 3 events: "Alice tagged Bob"); show active power-up icon + duration
- **Tron**: Add directional arrow showing facing; show "ELIMINATED" overlay text when dead

### 3C. Between-Rounds & Game-Over Polish (`web/ui.js`, `web/style.css`)
- Show cumulative leaderboard (not just last round) in between-rounds modal
- Add highlight for score changes since last round ("+2" deltas)
- Game Over: show match MVP with highlight treatment
- Add countdown progress bar (not just text "Next round in 8s")

### 3D. Spectator Mode Indicator (`bridge.rs`, `web/ui.js`)
- When `is_spectator = true`, show persistent "SPECTATOR" badge in HUD top-right
- Dim/hide controls hint since spectators can't input

**Estimated scope:** ~350 lines changed across 6-8 files.

---

## Phase 4: Responsiveness, Accessibility & Remaining Polish

**Goal:** Playable on mobile-width screens; keyboard-navigable throughout.

### 4A. Mobile Responsiveness (`web/style.css`)
- Add CSS media query breakpoints for `max-width: 600px`:
  - Stack game selector buttons 2x2 grid instead of 4-in-a-row
  - Increase touch target size to 44x44px minimum
  - Reduce HUD font sizes for narrow viewports
  - Move Tron gauges from bottom-right to bottom-center on narrow screens
  - Scale toast container width to `min(320px, 90vw)`

### 4B. Keyboard Navigation & Focus (`web/index.html`, `web/ui.js`, `web/style.css`)
- Add visible focus indicators on all interactive elements (`:focus-visible` outlines)
- Trap focus inside modals (between-rounds, game-over) when visible
- ESC key closes modals / dismisses toasts
- Tab order: name → game selector → create/join → room code

### 4C. Button Disabled States (`web/style.css`, `web/ui.js`)
- Use CSS `button:disabled` instead of inline `style.opacity` (currently in ui.js line 268)
- Add `cursor: not-allowed` for disabled buttons
- Disable "Start Game" for non-leaders with tooltip "Only the room leader can start"

### 4D. Stale Debug References (`web/debug.html`)
- (Already cleaned in Phase 4 of previous hardening — verify no regressions)

**Estimated scope:** ~200 lines changed across 4-5 files.

---

## Impact Summary

| Phase | Files | Focus | User Impact |
|-------|-------|-------|-------------|
| 1 | 8-10 | Loading, errors, audio | No more blank screens; games have sound |
| 2 | 8-10 | Visual feedback, polish | Games feel responsive and alive |
| 3 | 6-8 | HUD, toasts, scores | Players have full situational awareness |
| 4 | 4-5 | Mobile, keyboard, a11y | Accessible on all devices |

Total estimated: ~1,100 lines across ~15 unique files.
