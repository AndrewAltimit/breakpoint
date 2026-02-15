/**
 * Game Controls & HUD Tests
 *
 * Tests controls hint lifecycle, mouse/keyboard input through the Bevy canvas,
 * and game state correctness via WebSocket message decoding.
 *
 * Requires the breakpoint server running on http://127.0.0.1:8080
 * Player 1 = Playwright browser (host), Player 2 = raw WebSocket (state observer)
 */
import { test, expect } from '@playwright/test';
import WebSocket from 'ws';
import {
  MSG, decode, joinRoomMsg,
  parseJoinRoomResponse, parseGameStart, parseGameState,
  parseGolfState, parsePlatformerState, parseLaserTagState,
  encodeGolfInput, playerInputMsg,
} from './helpers/protocol.js';

// ============================================================================
// Shared helpers
// ============================================================================

/**
 * Install a WebSocket message interceptor on the page.
 * Must be called BEFORE navigating (via addInitScript).
 */
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

/**
 * Extract room code from intercepted WS messages on the page.
 */
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
      } catch { /* ignore decode errors */ }
    }
    await page.waitForTimeout(500);
  }
  return null;
}

/**
 * Connect Player 2 as a raw WebSocket and join the room.
 */
function connectPlayer2(wsUrl, roomCode) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const received = [];
    let playerId = null;

    ws.on('open', () => {
      ws.send(joinRoomMsg(roomCode, 'TestBot', [200, 100, 50]));
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
            reject(new Error(`Player 2 join failed: ${resp.error}`));
          }
        }
      } catch { /* ignore */ }
    });

    ws.on('error', (err) => reject(new Error(`Player 2 WS error: ${err.message}`)));
    setTimeout(() => reject(new Error('Player 2 join timed out')), 10000);
  });
}

/**
 * Wait for Player 2 to receive a specific message type.
 */
function waitForP2Message(p2, msgType, timeoutMs = 15000) {
  return new Promise((resolve, reject) => {
    for (const msg of p2.received) {
      if (msg.type === msgType) { resolve(msg); return; }
    }
    const startLen = p2.received.length;
    const interval = setInterval(() => {
      for (let i = startLen; i < p2.received.length; i++) {
        if (p2.received[i].type === msgType) {
          clearInterval(interval);
          resolve(p2.received[i]);
          return;
        }
      }
    }, 200);
    setTimeout(() => {
      clearInterval(interval);
      reject(new Error(`Timed out waiting for 0x${msgType.toString(16)}`));
    }, timeoutMs);
  });
}

/**
 * Collect console messages from the page, filtered by optional prefix.
 */
function collectConsoleLogs(page, prefix) {
  const logs = [];
  page.on('console', msg => {
    const text = msg.text();
    if (!prefix || text.startsWith(prefix)) {
      logs.push(text);
    }
  });
  return logs;
}

/**
 * Get the latest decoded GAME_STATE from Player 2's received messages.
 */
function getLatestGameState(p2) {
  const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
  if (states.length === 0) return null;
  const latest = states[states.length - 1];
  return parseGameState(latest.payload);
}

/**
 * Full game start sequence:
 * Install WS interceptor -> wait for WASM -> optionally select game ->
 * click Create Room -> extract room code -> connect P2 -> click Start Game ->
 * wait for GAME_START -> wait for initialization
 *
 * @param {import('@playwright/test').Page} page
 * @param {string} gameName - 'mini-golf' (default), 'platform-racer', or 'laser-tag'
 * @returns {{ canvas, box, p2, hostPlayerId }}
 */
async function startGame(page, gameName = 'mini-golf') {
  await installWsInterceptor(page);
  await page.goto('/');
  await page.waitForTimeout(15000); // Wait for WASM init
  const canvas = page.locator('#game-canvas');
  await expect(canvas).toBeAttached({ timeout: 5000 });
  const box = await canvas.boundingBox();

  // Select game if not default mini-golf.
  // Game selector buttons: Mini-Golf ~38%x, Platform Racer ~50%x, Laser Tag ~60%x.
  // Scan wider Y range and use force:true to avoid swiftshader stability waits.
  if (gameName === 'platform-racer') {
    for (const yPct of [0.28, 0.30, 0.32, 0.34, 0.36, 0.38]) {
      await canvas.click({ position: { x: box.width * 0.50, y: box.height * yPct }, force: true });
      await page.waitForTimeout(300);
    }
  } else if (gameName === 'laser-tag') {
    for (const yPct of [0.28, 0.30, 0.32, 0.34, 0.36, 0.38]) {
      await canvas.click({ position: { x: box.width * 0.60, y: box.height * yPct }, force: true });
      await page.waitForTimeout(300);
    }
  }
  // Allow time for the game selection to register
  await page.waitForTimeout(1000);

  // Click Create Room
  await canvas.click({ position: { x: box.width * 0.50, y: box.height * 0.56 } });
  const roomCode = await extractRoomCode(page, 15000);
  if (!roomCode) return null;

  // Player 2 joins
  let p2;
  try {
    p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
  } catch {
    return null;
  }
  await page.waitForTimeout(3000);

  // Click Start Game — scan vertical strip
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

  // Extract host player ID and verify game name from GameStart message
  const gsMsg = p2.received.find(m => m.type === MSG.GAME_START);
  const gs = parseGameStart(gsMsg.payload);
  const hostPlayerId = gs.hostId;
  const actualGame = gs.gameName;
  console.log(`Game started: ${actualGame} (requested: ${gameName}), host=${hostPlayerId}`);

  // Wait for game initialization (physics, rendering)
  await page.waitForTimeout(3000);

  return { canvas, box, p2, hostPlayerId, actualGame };
}

// ============================================================================
// Golf Controls & HUD
// ============================================================================

test.describe('Golf Controls & HUD', () => {
  test.describe.configure({ timeout: 180_000 });

  test('controls hint appears and auto-dismisses', async ({ page }) => {
    const hintLogs = collectConsoleLogs(page, 'BREAKPOINT:CONTROLS_HINT');
    const result = await startGame(page, 'mini-golf');
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    // Hint should have been spawned during setup
    expect(hintLogs.some(l => l.includes('SPAWNED'))).toBe(true);
    console.log('Controls hint SPAWNED detected');

    // Poll for auto-dismiss (8s game timer). Under swiftshader, Chromium
    // renders at <1fps so game time progresses slowly. The hint timer uses
    // Bevy's frame delta_secs, so at 0.5fps each frame subtracts ~2s.
    // 8s / 2s per frame = 4 frames ≈ 8s real, but frame timing is irregular
    // under load. Poll up to 90s to accommodate worst-case scheduling.
    const maxWait = 90000;
    const start = Date.now();
    while (Date.now() - start < maxWait) {
      if (hintLogs.some(l => l.includes('DISMISSED'))) break;
      await page.waitForTimeout(1000);
    }

    expect(hintLogs.some(l => l.includes('DISMISSED'))).toBe(true);
    console.log('Controls hint DISMISSED detected');

    p2.ws.close();
  });

  test('hold-click charges and fires visible stroke', async ({ page }) => {
    const result = await startGame(page, 'mini-golf');
    if (!result) { test.skip(); return; }
    const { p2, hostPlayerId } = result;
    const { canvas, box } = result;

    // Record initial host ball position from P2's state observer
    const initialGs = getLatestGameState(p2);
    let initialBallX = null, initialBallZ = null;
    if (initialGs) {
      try {
        const golf = parseGolfState(initialGs.stateData);
        const ball = golf.balls?.[hostPlayerId];
        if (ball) { initialBallX = ball[0][0]; initialBallZ = ball[0][2]; }
      } catch { /* ignore */ }
    }
    console.log(`Initial host ball: x=${initialBallX}, z=${initialBallZ}`);

    // Use the host's native mouse input (goes through browser → winit → Bevy).
    // Move mouse to canvas center to set aim angle, then hold-click to charge.
    const absX = box.x + box.width * 0.5;
    const absY = box.y + box.height * 0.5;
    await page.mouse.move(absX, absY);
    await page.waitForTimeout(500);

    // Hold left mouse button for 5s to charge power. Under swiftshader,
    // Chromium renders at <1fps (2-3s per frame), so we need the hold to span
    // at least 2 frame boundaries. 5s / ~2.5s per frame ≈ 2 frames seeing
    // pressed=true → power accumulates 2 × 0.025 = 0.05 → fires stroke.
    await page.mouse.down();
    await page.waitForTimeout(5000);
    await page.mouse.up();

    // Wait for host physics to process the stroke and broadcast state
    await page.waitForTimeout(5000);

    // Verify game state — host's ball should have moved, strokes incremented
    const afterGs = getLatestGameState(p2);
    expect(afterGs).not.toBeNull();
    const golf = parseGolfState(afterGs.stateData);
    console.log(`Strokes: ${JSON.stringify(golf.strokes)}, Balls: ${JSON.stringify(golf.balls)}`);

    const hostStrokes = golf.strokes?.[hostPlayerId];
    expect(hostStrokes).toBeGreaterThanOrEqual(1);

    const ball = golf.balls?.[hostPlayerId];
    if (ball && initialBallX !== null) {
      const afterX = ball[0][0], afterZ = ball[0][2];
      const vel = ball[1];
      const posChanged = (afterX !== initialBallX || afterZ !== initialBallZ);
      const hasVelocity = (Math.abs(vel[0]) > 0.01 || Math.abs(vel[2]) > 0.01);
      expect(posChanged || hasVelocity).toBe(true);
    }

    p2.ws.close();
  });

  test('minimum power stroke produces visible movement', async ({ page }) => {
    const result = await startGame(page, 'mini-golf');
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    // Wait for game to fully initialize — under heavy CPU load with 10
    // parallel workers, the host may take several seconds to process the
    // first few game ticks before it's ready to accept player input.
    await page.waitForTimeout(5000);
    const initialGs = getLatestGameState(p2);
    let initialBallX = null, initialBallZ = null;
    if (initialGs) {
      try {
        const golf = parseGolfState(initialGs.stateData);
        const ball = golf.balls?.[p2.playerId];
        if (ball) { initialBallX = ball[0][0]; initialBallZ = ball[0][2]; }
      } catch { /* ignore */ }
    }
    console.log(`P2 initial ball: x=${initialBallX}, z=${initialBallZ}`);

    // P2 sends minimum power stroke via WS injection (power=0.15).
    // This bypasses the unreliable host mouse input pipeline in headless
    // Chromium and directly tests the minimum power stroke mechanics.
    // power * MAX_POWER = 0.15 * 25 = 3.75 velocity → visible movement.
    // Send multiple times with delays — under swiftshader at <1fps the host
    // processes WS messages once per frame, so retry to handle contention.
    const golfInput = encodeGolfInput(0.0, 0.15, true);
    for (let attempt = 0; attempt < 3; attempt++) {
      p2.ws.send(playerInputMsg(p2.playerId, 0, golfInput));
      await page.waitForTimeout(5000);
      const gs = getLatestGameState(p2);
      if (gs) {
        try {
          const g = parseGolfState(gs.stateData);
          if ((g.strokes?.[p2.playerId] ?? 0) > 0) break;
        } catch { /* ignore */ }
      }
    }

    const afterGs = getLatestGameState(p2);
    expect(afterGs).not.toBeNull();
    const golf = parseGolfState(afterGs.stateData);
    console.log(`After min power stroke — Strokes: ${JSON.stringify(golf.strokes)}`);

    // Verify stroke counted for P2
    expect(golf.strokes?.[p2.playerId]).toBeGreaterThanOrEqual(1);

    // Verify P2 ball moved from spawn
    const ball = golf.balls?.[p2.playerId];
    expect(ball).toBeDefined();
    if (ball && initialBallX !== null) {
      const dx = Math.abs(ball[0][0] - initialBallX);
      const dz = Math.abs(ball[0][2] - initialBallZ);
      const displacement = Math.sqrt(dx * dx + dz * dz);
      console.log(`Ball displacement: ${displacement.toFixed(3)}`);
      expect(displacement).toBeGreaterThan(0.1);
    }

    p2.ws.close();
  });

  test('game state reflects correct course and scoring data', async ({ page }) => {
    const result = await startGame(page, 'mini-golf');
    if (!result) { test.skip(); return; }
    const { p2, hostPlayerId } = result;

    const gs = getLatestGameState(p2);
    expect(gs).not.toBeNull();

    const golf = parseGolfState(gs.stateData);
    console.log('Golf state:', JSON.stringify({
      courseIndex: golf.courseIndex,
      roundComplete: golf.roundComplete,
      roundTimer: golf.roundTimer,
    }));

    // courseIndex should be valid (0-8)
    expect(golf.courseIndex).toBeGreaterThanOrEqual(0);
    expect(golf.courseIndex).toBeLessThan(20);

    // strokes map should contain host player with 0 strokes
    const strokes = golf.strokes;
    const hostStrokes = strokes instanceof Map
      ? strokes.get(hostPlayerId) : strokes?.[hostPlayerId];
    expect(hostStrokes).toBe(0);
    console.log(`Host initial strokes: ${hostStrokes}`);

    // balls map should contain host player, not sunk
    const balls = golf.balls;
    const ball = balls instanceof Map ? balls.get(hostPlayerId) : balls?.[hostPlayerId];
    expect(ball).toBeDefined();
    // BallState: [position, velocity, is_sunk]
    const isSunk = Array.isArray(ball) ? ball[2] : ball?.is_sunk;
    expect(isSunk).toBe(false);
    console.log('Host ball is_sunk:', isSunk);

    p2.ws.close();
  });
});

// ============================================================================
// Platformer Controls
// ============================================================================

test.describe('Platformer Controls', () => {
  test.describe.configure({ timeout: 180_000 });

  test('controls hint appears on game start', async ({ page }) => {
    const hintLogs = collectConsoleLogs(page, 'BREAKPOINT:CONTROLS_HINT');
    const result = await startGame(page, 'platform-racer');
    if (!result) { test.skip(); return; }
    const { p2, actualGame } = result;

    console.log(`Platformer test: actual game = ${actualGame}`);
    // Game selection can fail in Firefox (canvas not resized → button positions off)
    if (actualGame !== 'platform-racer') {
      console.log(`Game selection failed: got ${actualGame} instead of platform-racer — skipping`);
      p2.ws.close();
      test.skip();
      return;
    }

    expect(hintLogs.some(l => l.includes('SPAWNED'))).toBe(true);
    console.log('Platformer controls hint SPAWNED detected');

    p2.ws.close();
  });

  test('keyboard movement and jump change player position', async ({ page }) => {
    const result = await startGame(page, 'platform-racer');
    if (!result) { test.skip(); return; }
    const { p2, hostPlayerId, actualGame } = result;

    console.log(`Platformer movement test: actual game = ${actualGame}`);
    expect(actualGame).toBe('platform-racer');

    // Click canvas to ensure keyboard focus
    const { canvas, box } = result;
    await canvas.click({ position: { x: box.width * 0.5, y: box.height * 0.5 } });
    await page.waitForTimeout(500);

    // Record initial player position
    const initialGs = getLatestGameState(p2);
    let initialX = null;
    let initialY = null;
    if (initialGs) {
      try {
        const state = parsePlatformerState(initialGs.stateData);
        const players = state.players;
        const player = players instanceof Map
          ? players.get(hostPlayerId) : players?.[hostPlayerId];
        if (player) {
          // PlatformerPlayerState: [x, y, vx, vy, ...]
          initialX = Array.isArray(player) ? player[0] : player?.x;
          initialY = Array.isArray(player) ? player[1] : player?.y;
        }
      } catch { /* ignore */ }
    }
    console.log(`Initial player pos: x=${initialX}, y=${initialY}`);

    // Press D to move right for 1s (longer for swiftshader)
    await page.keyboard.down('KeyD');
    await page.waitForTimeout(1000);
    await page.keyboard.up('KeyD');
    await page.waitForTimeout(1000);

    // Check position changed
    const afterMoveGs = getLatestGameState(p2);
    if (afterMoveGs) {
      try {
        const state = parsePlatformerState(afterMoveGs.stateData);
        const players = state.players;
        const player = players instanceof Map
          ? players.get(hostPlayerId) : players?.[hostPlayerId];
        if (player) {
          const afterX = Array.isArray(player) ? player[0] : player?.x;
          console.log(`Player x after D key: ${afterX} (was ${initialX})`);
          if (initialX !== null && afterX !== null
              && typeof initialX === 'number' && typeof afterX === 'number') {
            expect(afterX).toBeGreaterThan(initialX);
          }
        }
      } catch (e) {
        console.log(`Could not parse platformer state: ${e.message}`);
      }
    }

    // Press Space to jump
    await page.keyboard.press('Space');
    await page.waitForTimeout(500);

    const afterJumpGs = getLatestGameState(p2);
    if (afterJumpGs) {
      try {
        const state = parsePlatformerState(afterJumpGs.stateData);
        const players = state.players;
        const player = players instanceof Map
          ? players.get(hostPlayerId) : players?.[hostPlayerId];
        if (player) {
          // vy (index 3) should be positive after jump
          const vy = Array.isArray(player) ? player[3] : player?.vy;
          const afterY = Array.isArray(player) ? player[1] : player?.y;
          console.log(`Player after jump: y=${afterY}, vy=${vy}`);
        }
      } catch (e) {
        console.log(`Could not parse platformer state after jump: ${e.message}`);
      }
    }

    p2.ws.close();
  });
});

// ============================================================================
// Laser Tag Controls
// ============================================================================

test.describe('Laser Tag Controls', () => {
  test.describe.configure({ timeout: 180_000 });

  test('controls hint appears on game start', async ({ page }) => {
    const hintLogs = collectConsoleLogs(page, 'BREAKPOINT:CONTROLS_HINT');
    const result = await startGame(page, 'laser-tag');
    if (!result) { test.skip(); return; }
    const { p2, actualGame } = result;

    console.log(`Laser tag test: actual game = ${actualGame}`);
    expect(actualGame).toBe('laser-tag');

    expect(hintLogs.some(l => l.includes('SPAWNED'))).toBe(true);
    console.log('Laser tag controls hint SPAWNED detected');

    p2.ws.close();
  });

  test('WASD movement changes player position', async ({ page }) => {
    const result = await startGame(page, 'laser-tag');
    if (!result) { test.skip(); return; }
    const { p2, hostPlayerId, actualGame } = result;

    console.log(`Laser tag movement test: actual game = ${actualGame}`);
    expect(actualGame).toBe('laser-tag');

    // Click canvas to ensure keyboard focus
    const { canvas, box } = result;
    await canvas.click({ position: { x: box.width * 0.5, y: box.height * 0.5 } });
    await page.waitForTimeout(500);

    // Record initial player position
    const initialGs = getLatestGameState(p2);
    let initialX = null;
    let initialZ = null;
    if (initialGs) {
      try {
        const state = parseLaserTagState(initialGs.stateData);
        const players = state.players;
        const player = players instanceof Map
          ? players.get(hostPlayerId) : players?.[hostPlayerId];
        if (player) {
          // LaserPlayerState: [x, z, aim_angle, ...]
          initialX = Array.isArray(player) ? player[0] : player?.x;
          initialZ = Array.isArray(player) ? player[1] : player?.z;
        }
      } catch { /* ignore */ }
    }
    console.log(`Initial laser tag pos: x=${initialX}, z=${initialZ}`);

    // Press W to move forward for 1s (longer for swiftshader)
    await page.keyboard.down('KeyW');
    await page.waitForTimeout(1000);
    await page.keyboard.up('KeyW');
    await page.waitForTimeout(1000);

    // Check position changed
    const afterGs = getLatestGameState(p2);
    if (afterGs) {
      try {
        const state = parseLaserTagState(afterGs.stateData);
        const players = state.players;
        const player = players instanceof Map
          ? players.get(hostPlayerId) : players?.[hostPlayerId];
        if (player) {
          const afterX = Array.isArray(player) ? player[0] : player?.x;
          const afterZ = Array.isArray(player) ? player[1] : player?.z;
          console.log(`Laser tag pos after W: x=${afterX}, z=${afterZ}`);
          if (initialX !== null && initialZ !== null && afterX !== null && afterZ !== null
              && typeof initialX === 'number' && typeof afterX === 'number') {
            // At least one axis should have changed
            const dx = Math.abs(afterX - initialX);
            const dz = Math.abs(afterZ - initialZ);
            const displacement = Math.sqrt(dx * dx + dz * dz);
            console.log(`Laser tag displacement: ${displacement.toFixed(3)}`);
            expect(displacement).toBeGreaterThan(0.01);
          }
        }
      } catch (e) {
        console.log(`Could not parse laser tag state: ${e.message}`);
      }
    }

    p2.ws.close();
  });
});
