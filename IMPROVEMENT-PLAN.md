# Breakpoint Comprehensive Improvement Plan

Based on thorough exploration of all 9 workspace crates, web UI, tests, documentation, CI/CD, and Docker configuration (484 tests, ~25,000 lines of Rust, ~5,000 lines of JS/CSS/HTML).

---

## Phase 1: Bug Fixes & Safety Hardening

**Goal:** Fix actual bugs and safety issues that affect correctness or could cause runtime failures.

### 1.1 Fix `unwrap()` calls in production code paths (room_manager.rs)
- **File:** `crates/breakpoint-server/src/room_manager.rs`
- **Issue:** `reconnect()` uses `.lock().unwrap()` on a Mutex (line ~223), and `add_bot()` uses `unwrap()` after validation (line ~332). These violate the otherwise excellent "no unwrap in library code" pattern.
- **Fix:** Replace with `.lock().map_err(...)` or `if let Ok(mut senders)` pattern.

### 1.2 Fix IpConnectionGuard drop race condition (state.rs)
- **File:** `crates/breakpoint-server/src/state.rs`
- **Issue:** Lines 111-119 — `IpConnectionGuard::drop()` spawns a tokio task to decrement the connection counter. If tokio is shutting down or two guards drop simultaneously, the counter can leak (never decremented).
- **Fix:** Use `Arc<AtomicUsize>` with synchronous `fetch_sub` in the Drop impl instead of spawning an async task.

### 1.3 Fix Tron minimap unbounded wall segment growth (bridge.rs)
- **File:** `crates/breakpoint-client/src/bridge.rs`
- **Issue:** Lines 381-398 — wall segments array serialized to JSON for minimap grows linearly with game duration (at 20Hz tick rate, ~2400 segments after 2 minutes). Never pruned.
- **Fix:** Cap serialized wall segments (e.g., last 500) or implement incremental updates (send only new segments since last frame).

### 1.4 Add WebSocket connection timeout (net_client.rs)
- **File:** `crates/breakpoint-client/src/net_client.rs`
- **Issue:** `connect()` initiates WebSocket but never times out. If the server never sends `onopen`, the client hangs indefinitely, and the reconnection logic in `app.rs` never fires.
- **Fix:** Add a JS `setTimeout` that closes the WebSocket and triggers error state if `onopen` doesn't fire within 10 seconds.

### 1.5 Preserve outbound queue across reconnection (net_client.rs)
- **File:** `crates/breakpoint-client/src/net_client.rs`
- **Issue:** Line 167 — outbound message queue is cleared on disconnect, dropping any player inputs sent just before disconnection.
- **Fix:** Retain the outbound queue and flush it after successful reconnection (with staleness check — discard messages older than 2 seconds).

### 1.6 Fix session token reconnection logic (app.rs)
- **File:** `crates/breakpoint-client/src/app.rs`
- **Issue:** Line ~359 — uses `.unwrap_or_default()` for session token during reconnection, which could lose the token if deserialization fails. Token is already stored in `self.session_token`.
- **Fix:** Use the already-stored `self.session_token.clone()` instead of re-deriving from message.

### 1.7 Log broadcast errors in event store (event_store.rs)
- **File:** `crates/breakpoint-server/src/event_store.rs`
- **Issue:** Line 66 — `let _ = self.broadcast_tx.send(event.clone())` silently ignores broadcast failures, making overload conditions invisible.
- **Fix:** Log a warning when broadcast fails (lagged subscribers).

---

## Phase 2: Performance Optimizations

**Goal:** Address performance bottlenecks, especially for Tron (the most demanding game with 500+ wall segments).

### 2.1 Add frustum culling to renderer (renderer.rs)
- **File:** `crates/breakpoint-client/src/renderer.rs`
- **Issue:** All scene objects are rendered regardless of camera visibility. For Tron with 500+ wall segments, many are off-screen, wasting draw calls.
- **Fix:** Implement simple AABB-based frustum culling using the camera's view-projection matrix. Skip `gl.draw_arrays()` for objects outside the frustum.

### 2.2 Add object pooling to scene graph (scene.rs)
- **File:** `crates/breakpoint-client/src/scene.rs`
- **Issue:** Scene is rebuilt from scratch every frame (`clear()` then re-add). For Tron, this means 500+ allocations per frame.
- **Fix:** Implement a `reset_and_reuse()` method that marks objects as inactive rather than deallocating, then reuse slots. Use `Vec::with_capacity()` based on previous frame's object count.

### 2.3 Optimize scene ID lookup for large scenes (scene.rs)
- **File:** `crates/breakpoint-client/src/scene.rs`
- **Issue:** `get_mut()` (line ~132) uses linear search `iter_mut().find()`, which is O(n) per lookup. For Tron scenes with 500+ objects, this becomes a bottleneck.
- **Fix:** Add a `HashMap<ObjectId, usize>` index for O(1) lookups when scene size exceeds a threshold (e.g., 64 objects).

### 2.4 Reduce JSON bridge payload for Tron (bridge.rs)
- **File:** `crates/breakpoint-client/src/bridge.rs`
- **Issue:** Full UI state (15-20KB for Tron) is JSON-serialized every frame via `push_ui_state()`. At 60fps, that's ~1MB/s of JSON serialization.
- **Fix:** Implement dirty-flag system — only serialize and push when state actually changes. For Tron minimap, only send new wall segments (delta encoding).

### 2.5 Use `Vec::with_capacity()` for known-size allocations
- **Files:** Multiple (scene.rs, bridge.rs, game render files)
- **Issue:** Several `Vec::new()` calls where the approximate capacity is known (e.g., player count, wall segment count).
- **Fix:** Replace with `Vec::with_capacity(estimated_count)` to reduce reallocations.

---

## Phase 3: Game Balance & Gameplay Improvements

**Goal:** Fix gameplay balance issues and add missing mechanics identified across all four games.

### 3.1 LaserTag: Add post-stun invulnerability frames
- **File:** `crates/games/breakpoint-lasertag/src/lib.rs`
- **Issue:** Players can be stunned immediately after unstunning, enabling perma-stun in 1vN situations. No counter-play exists.
- **Fix:** Add 1.0s invulnerability after stun expires. During invulnerability, player is semi-transparent and cannot be hit.

### 3.2 LaserTag: Implement line-of-sight blocking for smoke zones
- **File:** `crates/games/breakpoint-lasertag/src/projectile.rs`
- **Issue:** Smoke zones are visual only — lasers pass right through them. Players expect smoke to provide cover, which is misleading.
- **Fix:** Add smoke zones to the raycast check. Lasers entering smoke have reduced range (e.g., 50% remaining distance consumed) or are fully blocked.

### 3.3 Tron: Improve bot AI with multi-step lookahead
- **File:** `crates/games/breakpoint-tron/src/bot.rs`
- **Issue:** Bot evaluates only 3 fixed directions (straight/left/right) with single-step lookahead. Gets cornered easily, doesn't grind walls strategically.
- **Fix:** Add 2-step lookahead (evaluate 3 directions, then for the best 2, evaluate 3 more directions each = 9 scenarios). Add wall-proximity awareness for grinding bonus.

### 3.4 Tron: Add stress tests for grinding mechanic
- **File:** `crates/games/breakpoint-tron/src/` (new test)
- **Issue:** No tests verify the grinding (wall acceleration) mechanic or speed cap behavior.
- **Fix:** Add tests for: grinding near own wall, grinding near enemy wall, speed cap at max_speed, deceleration when leaving grind range.

### 3.5 Platformer: Clarify DoubleJump behavior on respawn
- **File:** `crates/games/breakpoint-platformer/src/lib.rs`
- **Issue:** When a player with DoubleJump power-up respawns, `has_double_jump` is reset to false (physics.rs line ~133), but the power-up slot isn't consumed. Player loses benefit without consuming the power-up.
- **Fix:** Either consume the power-up on respawn (clearing the slot) or preserve `has_double_jump = true` through respawn.

---

## Phase 4: Web UI & Accessibility

**Goal:** Improve web UI quality, accessibility compliance (WCAG 2.1 AA), and mobile experience.

### 4.1 Increase touch targets to WCAG minimum (style.css)
- **File:** `web/style.css`
- **Issue:** Icon buttons (#btn-mute, #btn-dashboard) are 40x40px, below the 44x44px WCAG 2.5.5 recommendation.
- **Fix:** Increase to 44x44px minimum with appropriate padding.

### 4.2 Add focus trap to modal dialogs (ui.js)
- **File:** `web/ui.js`
- **Issue:** Modal dialogs (game-over, settings) lack focus trapping — Tab can escape to background elements.
- **Fix:** Implement focus trap: on modal open, find all focusable elements within modal, trap Tab/Shift+Tab cycling.

### 4.3 Add `aria-atomic` to toast container (index.html)
- **File:** `web/index.html`
- **Issue:** Toast container has `aria-label` but not `aria-atomic="true"`, so screen readers may not announce new toasts.
- **Fix:** Add `aria-atomic="true"` and `aria-relevant="additions"` to the toast container.

### 4.4 Add `lang` attribute and skip-to-content link (index.html)
- **File:** `web/index.html`
- **Issue:** Missing `lang="en"` on `<html>` element. No skip-to-content link for keyboard users.
- **Fix:** Add `lang="en"` and a visually-hidden skip link at the top of body.

### 4.5 Add debouncing to UI actions (ui.js)
- **File:** `web/ui.js`
- **Issue:** Create/Join room buttons have no debounce — double-clicking creates duplicate rooms or sends duplicate join requests.
- **Fix:** Add 500ms debounce on Create/Join clicks. Disable buttons during pending operations.

### 4.6 Reduce toast animation in prefers-reduced-motion (style.css)
- **File:** `web/style.css`
- **Issue:** Toast slide-in/slide-out animations still play at 0.3s even when user has `prefers-reduced-motion` enabled.
- **Fix:** Set `animation-duration: 0.01s` within the `@media (prefers-reduced-motion: reduce)` block.

### 4.7 Add tablet breakpoint (style.css)
- **File:** `web/style.css`
- **Issue:** Only one mobile breakpoint at 600px. No optimization for tablets (768px-1024px landscape).
- **Fix:** Add `@media (max-width: 1024px)` breakpoint with adjusted game selector layout and HUD sizing.

---

## Phase 5: Server Robustness & Protocol Improvements

**Goal:** Harden server against edge cases and improve protocol resilience.

### 5.1 Add rate-limit feedback to WebSocket clients (ws.rs)
- **File:** `crates/breakpoint-server/src/ws.rs`
- **Issue:** Lines 328-329 — rate-limited messages are silently dropped with only a server-side warn log. Client has no indication their inputs are being throttled.
- **Fix:** Send a `RateLimited` message type back to the client when inputs are dropped, allowing the client to display a warning.

### 5.2 Add first-message rejection logging (ws.rs)
- **File:** `crates/breakpoint-server/src/ws.rs`
- **Issue:** Lines 64-66 — if the first WebSocket message isn't a valid JoinRoom, the connection is silently dropped with no logging.
- **Fix:** Log the rejected message type/size at `warn` level for debugging.

### 5.3 Add relay backpressure and client limits (relay.rs)
- **File:** `crates/breakpoint-relay/src/relay.rs`
- **Issue:** No maximum clients per room limit. `try_send()` silently drops messages to slow readers. No backpressure mechanism.
- **Fix:** Add configurable `max_clients_per_room` (default: 16). Track dropped messages per client and disconnect after threshold.

### 5.4 Fix GitHub poller memory leak (poller.rs)
- **File:** `crates/adapters/breakpoint-github/src/poller.rs`
- **Issue:** `active_runs` HashMap grows indefinitely — completed workflow runs are never pruned. `fail_24h` and `success_24h` counters never reset.
- **Fix:** Prune `active_runs` entries older than 24h on each poll cycle. Reset aggregate counters daily or use sliding window.

### 5.5 Defensive JSON navigation in GitHub webhooks (webhooks/github.rs)
- **File:** `crates/breakpoint-server/src/webhooks/github.rs`
- **Issue:** Lines 77-81 — chained JSON field access (`payload["sender"]["login"]`) without null-safety. Uses `.as_str().unwrap_or()` but could be more defensive.
- **Fix:** Use `.get("sender").and_then(|s| s.get("login")).and_then(|l| l.as_str()).unwrap_or("unknown")` pattern consistently.

---

## Phase 6: Test Coverage Expansion

**Goal:** Close identified test coverage gaps, especially in undertested game crates and server features.

### 6.1 Add SSE stream integration test
- **File:** `crates/breakpoint-server/tests/` (new file: `sse_integration.rs`)
- **Issue:** SSE endpoint (`GET /api/v1/events/stream`) has no integration test.
- **Test:** Connect SSE client, post event via API, verify SSE receives it within 1s.

### 6.2 Add rate limiting integration tests
- **File:** `crates/breakpoint-server/tests/api_integration.rs` (extend)
- **Issue:** Rate limiting behavior is untested at integration level.
- **Test:** Send events at 3x the configured rate limit, verify 429 responses after bucket exhaustion.

### 6.3 Add Tron grinding mechanic tests
- **File:** `crates/games/breakpoint-tron/src/lib.rs` (extend `#[cfg(test)]`)
- **Issue:** No tests for wall-grinding acceleration or speed cap.
- **Tests:** Near-wall acceleration, speed cap enforcement, deceleration when leaving grind range.

### 6.4 Add relay stress test
- **File:** `crates/breakpoint-relay/tests/` (new)
- **Issue:** Only 3 basic relay tests. No multi-client or throughput testing.
- **Tests:** 8 simultaneous clients, rapid message forwarding, disconnect/reconnect under load.

### 6.5 Add property-based tests for game physics
- **Files:** `crates/games/breakpoint-golf/src/physics.rs`, `breakpoint-platformer/src/physics.rs`
- **Issue:** `proptest` is in workspace dependencies but unused beyond golf.
- **Tests:** Add proptest strategies for: random ball positions never escape bounds, random platform positions always have valid collisions, laser reflections maintain energy conservation.

### 6.6 Add event claim timeout test
- **File:** `crates/breakpoint-server/tests/api_integration.rs` (extend)
- **Issue:** Event claim and re-surface logic untested.
- **Test:** Claim an event, verify it becomes pending again after timeout expires.

---

## Phase Summary

| Phase | Items | Focus | Risk Level |
|-------|-------|-------|------------|
| **Phase 1** | 7 | Bug fixes & safety | Low (defensive fixes) |
| **Phase 2** | 5 | Performance | Medium (renderer changes) |
| **Phase 3** | 5 | Game balance | Medium (gameplay changes) |
| **Phase 4** | 7 | UI & accessibility | Low (CSS/HTML/JS) |
| **Phase 5** | 5 | Server robustness | Medium (protocol changes) |
| **Phase 6** | 6 | Test coverage | Low (additive tests) |

**Total: 35 improvement items across 6 phases**

---

## Recommended Implementation Order

1. **Phase 1** first — fixes real bugs, minimal risk
2. **Phase 6** second — tests validate existing behavior before changes
3. **Phase 4** third — UI/accessibility improvements are self-contained
4. **Phase 2** fourth — performance work benefits from test coverage (Phase 6)
5. **Phase 5** fifth — server hardening with test validation
6. **Phase 3** last — gameplay changes need playtesting and are most subjective
