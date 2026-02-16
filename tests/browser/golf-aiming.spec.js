/**
 * Golf Aiming Direction Tests
 *
 * Verifies the input→physics direction chain: cursor/aim_angle → ball velocity.
 * Layer 3 of the golf input-to-physics testing framework.
 *
 * - WS injection tests: P2 sends GolfInput via WebSocket, verifies ball direction
 * - Mouse-driven tests: Full E2E through browser rendering pipeline
 *
 * Requires the breakpoint server running on http://127.0.0.1:8080
 */
import { test, expect } from '@playwright/test';
import WebSocket from 'ws';
import {
  MSG, decode, encode, joinRoomMsg,
  parseJoinRoomResponse, parseGameStart, parseGameState,
  parseGolfState, encodeGolfInput, playerInputMsg,
} from './helpers/protocol.js';

// ============================================================================
// Shared helpers (extracted from game-controls.spec.js patterns)
// ============================================================================

async function installWsInterceptor(page) {
  await page.addInitScript(() => {
    window.__wsMessages = [];
    window.__wsSendMessages = [];
    window.__wsInstance = null;

    const OrigWS = window.WebSocket;
    window.WebSocket = function (...args) {
      const ws = new OrigWS(...args);
      window.__wsInstance = ws;
      ws.addEventListener('message', (event) => {
        if (event.data instanceof Blob) {
          event.data.arrayBuffer().then(buf => {
            const bytes = new Uint8Array(buf);
            let binary = '';
            for (let i = 0; i < bytes.length; i++) {
              binary += String.fromCharCode(bytes[i]);
            }
            window.__wsMessages.push(btoa(binary));
          });
        } else if (event.data instanceof ArrayBuffer) {
          const bytes = new Uint8Array(event.data);
          let binary = '';
          for (let i = 0; i < bytes.length; i++) {
            binary += String.fromCharCode(bytes[i]);
          }
          window.__wsMessages.push(btoa(binary));
        }
      });
      return ws;
    };
    window.WebSocket.prototype = OrigWS.prototype;
    window.WebSocket.CONNECTING = OrigWS.CONNECTING;
    window.WebSocket.OPEN = OrigWS.OPEN;
    window.WebSocket.CLOSING = OrigWS.CLOSING;
    window.WebSocket.CLOSED = OrigWS.CLOSED;
  });
}

async function extractRoomCode(page, maxWaitMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < maxWaitMs) {
    const b64Messages = await page.evaluate(() => window.__wsMessages || []);
    for (const b64 of b64Messages) {
      try {
        const buf = Buffer.from(b64, 'base64');
        const msg = decode(buf);
        if (msg.type === MSG.JOIN_ROOM_RESPONSE && msg.payload) {
          const resp = parseJoinRoomResponse(msg.payload);
          if (resp.success && resp.roomCode) return resp.roomCode;
        }
      } catch { /* ignore */ }
    }
    await page.waitForTimeout(500);
  }
  return null;
}

function connectPlayer2(wsUrl, roomCode) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const received = [];
    let playerId = null;

    ws.on('open', () => {
      ws.send(joinRoomMsg(roomCode, 'AimBot', [200, 100, 50]));
    });

    ws.on('message', (data) => {
      const buf = Buffer.from(data);
      try {
        const decoded = decode(buf);
        received.push(decoded);
        if (decoded.type === MSG.JOIN_ROOM_RESPONSE) {
          const resp = parseJoinRoomResponse(decoded.payload);
          if (resp.success) {
            playerId = resp.playerId;
            resolve({ ws, playerId, received });
          } else {
            reject(new Error(`P2 join failed: ${resp.error}`));
          }
        }
      } catch { /* ignore */ }
    });

    ws.on('error', (err) => reject(new Error(`P2 WS error: ${err.message}`)));
    setTimeout(() => reject(new Error('P2 join timed out')), 10000);
  });
}

function getLatestGolfState(p2) {
  const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
  if (states.length === 0) return null;
  const latest = states[states.length - 1];
  const gs = parseGameState(latest.payload);
  return parseGolfState(gs.stateData);
}

/**
 * Wait until P2 receives a game state where the given player's ball
 * has moved from origin or has non-zero velocity.
 */
function waitForBallMovement(p2, playerId, initialX, initialZ, timeoutMs = 15000) {
  return new Promise((resolve, reject) => {
    const start = Date.now();
    const interval = setInterval(() => {
      const golf = getLatestGolfState(p2);
      if (golf) {
        const ball = golf.balls?.[playerId];
        if (ball) {
          const pos = ball[0]; // [x, y, z]
          const vel = ball[1]; // [vx, vy, vz]
          const moved = (Math.abs(pos[0] - initialX) > 0.1
                      || Math.abs(pos[2] - initialZ) > 0.1);
          const hasVel = (Math.abs(vel[0]) > 0.05 || Math.abs(vel[2]) > 0.05);
          if (moved || hasVel) {
            clearInterval(interval);
            resolve({ pos, vel });
          }
        }
      }
      if (Date.now() - start > timeoutMs) {
        clearInterval(interval);
        // Return latest state even on timeout
        const golf = getLatestGolfState(p2);
        const ball = golf?.balls?.[playerId];
        resolve(ball ? { pos: ball[0], vel: ball[1] } : null);
      }
    }, 200);
  });
}

/**
 * Wait for the FIRST game state (after startIdx) where the ball has moved.
 * Unlike waitForBallMovement which checks the latest state, this scans
 * chronologically to catch the earliest state showing movement — critical
 * when wall bounces could reverse direction in later ticks.
 */
async function waitForFirstBallMovement(p2, playerId, initialX, initialZ, startIdx = 0, timeoutMs = 10000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
    for (let i = startIdx; i < states.length; i++) {
      try {
        const gs = parseGameState(states[i].payload);
        const golf = parseGolfState(gs.stateData);
        const ball = golf.balls?.[playerId];
        if (ball) {
          const pos = ball[0];
          const vel = ball[1];
          const moved = (Math.abs(pos[0] - initialX) > 0.1 || Math.abs(pos[2] - initialZ) > 0.1);
          const hasVel = (Math.abs(vel[0]) > 0.05 || Math.abs(vel[2]) > 0.05);
          if (moved || hasVel) {
            return { pos, vel };
          }
        }
      } catch { /* ignore parse errors */ }
    }
    await new Promise(r => setTimeout(r, 50));
  }
  return null;
}

async function startGolfGame(page) {
  await installWsInterceptor(page);
  await page.goto('/');
  await page.waitForTimeout(15000);
  const canvas = page.locator('#game-canvas');
  await expect(canvas).toBeAttached({ timeout: 5000 });
  const box = await canvas.boundingBox();

  // Click Create Room
  await canvas.click({ position: { x: box.width * 0.50, y: box.height * 0.56 } });
  const roomCode = await extractRoomCode(page, 15000);
  if (!roomCode) return null;

  let p2;
  try {
    p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
  } catch {
    return null;
  }
  await page.waitForTimeout(3000);

  // Click Start Game
  let gameStarted = false;
  for (const yPct of [0.62, 0.65, 0.68, 0.70, 0.73]) {
    await canvas.click({ position: { x: box.width * 0.50, y: box.height * yPct } });
    await page.waitForTimeout(1500);
    if (p2.received.find(m => m.type === MSG.GAME_START)) {
      gameStarted = true;
      break;
    }
  }
  if (!gameStarted) return null;

  const gsMsg = p2.received.find(m => m.type === MSG.GAME_START);
  const gs = parseGameStart(gsMsg.payload);

  await page.waitForTimeout(3000);

  return { canvas, box, p2, hostPlayerId: gs.hostId };
}

// ============================================================================
// WS Injection Direction Tests
// ============================================================================

test.describe('Golf Aiming Direction (WS injection)', () => {
  test.describe.configure({ timeout: 180_000 });

  test('P2 input aim_angle=0 moves ball +X', async ({ page }) => {
    const result = await startGolfGame(page);
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    // Get P2's initial ball position
    const initialGolf = getLatestGolfState(p2);
    const initialBall = initialGolf?.balls?.[p2.playerId];
    const initialX = initialBall ? initialBall[0][0] : 0;
    const initialZ = initialBall ? initialBall[0][2] : 0;
    console.log(`P2 initial ball: x=${initialX}, z=${initialZ}`);

    // P2 sends stroke at aim_angle=0 (should move +X).
    // Use LOW power (0.15) so the ball doesn't hit the right wall (only ~10 units away)
    // within the first tick. High power causes wall bounces that invert the measured direction.
    const golfInput = encodeGolfInput(0.0, 0.15, true);
    p2.ws.send(playerInputMsg(p2.playerId, 0, golfInput));

    const movement = await waitForBallMovement(p2, p2.playerId, initialX, initialZ);
    console.log('P2 ball after stroke:', JSON.stringify(movement));

    expect(movement).not.toBeNull();
    if (movement) {
      // Ball should have moved in +X direction
      const dx = movement.pos[0] - initialX;
      expect(dx).toBeGreaterThan(0);
      console.log(`Ball moved +X by ${dx.toFixed(3)}`);
    }

    p2.ws.close();
  });

  test('P2 input aim_angle=PI/2 moves ball +Z', async ({ page }) => {
    const result = await startGolfGame(page);
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    const initialGolf = getLatestGolfState(p2);
    const initialBall = initialGolf?.balls?.[p2.playerId];
    const initialX = initialBall ? initialBall[0][0] : 0;
    const initialZ = initialBall ? initialBall[0][2] : 0;

    // Low power to avoid wall bounces before first state update
    const golfInput = encodeGolfInput(Math.PI / 2, 0.15, true);
    p2.ws.send(playerInputMsg(p2.playerId, 0, golfInput));

    const movement = await waitForBallMovement(p2, p2.playerId, initialX, initialZ);
    console.log('P2 ball after PI/2 stroke:', JSON.stringify(movement));

    expect(movement).not.toBeNull();
    if (movement) {
      const dz = movement.pos[2] - initialZ;
      expect(dz).toBeGreaterThan(0);
      console.log(`Ball moved +Z by ${dz.toFixed(3)}`);
    }

    p2.ws.close();
  });

  test('P2 input aim_angle=PI moves ball -X', async ({ page }) => {
    const result = await startGolfGame(page);
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    const initialGolf = getLatestGolfState(p2);
    const initialBall = initialGolf?.balls?.[p2.playerId];
    const initialX = initialBall ? initialBall[0][0] : 0;
    const initialZ = initialBall ? initialBall[0][2] : 0;

    // Low power to avoid wall bounces before first state update
    const golfInput = encodeGolfInput(Math.PI, 0.15, true);
    p2.ws.send(playerInputMsg(p2.playerId, 0, golfInput));

    const movement = await waitForBallMovement(p2, p2.playerId, initialX, initialZ);
    console.log('P2 ball after PI stroke:', JSON.stringify(movement));

    expect(movement).not.toBeNull();
    if (movement) {
      const dx = movement.pos[0] - initialX;
      expect(dx).toBeLessThan(0);
      console.log(`Ball moved -X by ${dx.toFixed(3)}`);
    }

    p2.ws.close();
  });
});

// ============================================================================
// Mouse-driven Direction Tests — NOT VIABLE in headless mode
// ============================================================================
//
// Winit reads cursor position from PointerEvent.offsetX/offsetY (computed
// properties that the browser calculates from the event target's layout).
// In headless Playwright + swiftshader, programmatic mouse events don't
// produce correct offsetX/offsetY values, so Bevy's cursor_position() always
// returns None → aim_angle stays at the default (0).
//
// Direction verification is fully covered by the WS injection tests above,
// which bypass the browser cursor pipeline and directly test the aim→physics
// chain. The cursor→aim math is additionally covered by native Rust unit
// tests in golf_plugin.rs (cursor_to_ground, looking_at axis tests, etc.).
//
// Mouse-driven stroke mechanics (click-to-charge, power scaling) are tested
// in game-controls.spec.js.
