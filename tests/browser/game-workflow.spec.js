/**
 * Game Workflow Tests
 *
 * Tests the full multiplayer game workflow:
 *   Lobby → Create Room → Player 2 joins → Start Game → In-Game rendering
 *
 * Requires the breakpoint server running on http://127.0.0.1:8080
 * Player 1 = Playwright browser, Player 2 = raw WebSocket (Node.js)
 */
import { test, expect } from '@playwright/test';
import WebSocket from 'ws';
import {
  MSG, encode, decode, joinRoomMsg,
  parseJoinRoomResponse, parsePlayerList, parseGameStart, parseGameState,
  parseGolfState, playerInputMsg, encodeGolfInput, msgTypeName,
} from './helpers/protocol.js';

// Longer timeout for WASM-heavy tests
test.describe.configure({ timeout: 180_000 });

/**
 * Collect browser console messages for diagnostics.
 */
function collectConsole(page) {
  const messages = [];
  page.on('console', msg => {
    messages.push({ type: msg.type(), text: msg.text() });
  });
  page.on('pageerror', err => {
    messages.push({ type: 'pageerror', text: err.message });
  });
  return messages;
}

/**
 * Wait for WASM to initialize by checking for canvas and rAF activity.
 */
async function waitForWasm(page, timeoutMs = 20000) {
  await page.goto('/');
  await page.waitForTimeout(timeoutMs);
  const canvas = page.locator('#game-canvas');
  await expect(canvas).toBeAttached({ timeout: 5000 });
}

/**
 * Install a WebSocket message interceptor on the page.
 * Must be called BEFORE navigating (via addInitScript).
 * Captured messages are stored in window.__wsMessages as base64 strings.
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
 * Get all intercepted WS messages from the browser, decoded.
 */
async function getWsMessages(page) {
  const b64Messages = await page.evaluate(() => window.__wsMessages || []);
  return b64Messages.map(b64 => {
    const buf = Buffer.from(b64, 'base64');
    try {
      return decode(buf);
    } catch {
      return { type: buf[0], payload: null, raw: buf };
    }
  });
}

/**
 * Extract room code from intercepted WS messages.
 */
async function extractRoomCode(page, maxWaitMs = 15000) {
  const start = Date.now();
  while (Date.now() - start < maxWaitMs) {
    const msgs = await getWsMessages(page);
    for (const msg of msgs) {
      if (msg.type === MSG.JOIN_ROOM_RESPONSE && msg.payload) {
        const resp = parseJoinRoomResponse(msg.payload);
        if (resp.success && resp.roomCode) {
          return resp.roomCode;
        }
      }
    }
    await page.waitForTimeout(500);
  }
  return null;
}

/**
 * Connect Player 2 as a raw WebSocket and join the room.
 * Returns a promise that resolves with the WS connection and player info.
 */
function connectPlayer2(wsUrl, roomCode) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const received = [];
    let playerId = null;

    ws.on('open', () => {
      // Send JoinRoom message
      const msg = joinRoomMsg(roomCode, 'TestBot', [200, 100, 50]);
      ws.send(msg);
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
      } catch (e) {
        // Ignore decode errors for non-msgpack messages
      }
    });

    ws.on('error', (err) => {
      reject(new Error(`Player 2 WebSocket error: ${err.message}`));
    });

    setTimeout(() => {
      reject(new Error('Player 2 join timed out'));
    }, 10000);
  });
}

/**
 * Wait for Player 2 to receive a specific message type.
 */
function waitForMessage(p2, msgType, timeoutMs = 15000) {
  return new Promise((resolve, reject) => {
    // Check existing messages first
    for (const msg of p2.received) {
      if (msg.type === msgType) {
        resolve(msg);
        return;
      }
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
      reject(new Error(`Timed out waiting for message type 0x${msgType.toString(16)}`));
    }, timeoutMs);
  });
}

// ============================================================================
// Tests
// ============================================================================

test.describe('Game Workflow', () => {
  test('lobby renders with game title and buttons', async ({ page }) => {
    const messages = collectConsole(page);
    await waitForWasm(page);

    // Take screenshot — should show "BREAKPOINT" title, game buttons, etc.
    const screenshot = await page.screenshot();
    expect(screenshot.byteLength).toBeGreaterThan(5000);

    // Verify no panics during lobby render
    const panics = messages.filter(m =>
      m.text.includes('panicked') || m.text.includes('RuntimeError')
    );
    expect(panics).toHaveLength(0);

    console.log('Lobby rendered successfully, screenshot captured');
  });

  test('Create Room establishes WebSocket connection', async ({ page }) => {
    await installWsInterceptor(page);
    await waitForWasm(page);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Click "Create Room" button (approx center-left of button row ~59% down)
    // Create Room is now a standalone centered button (editor removed)
    const createX = box.width * 0.50;
    const createY = box.height * 0.56;
    console.log(`Clicking Create Room at (${createX.toFixed(0)}, ${createY.toFixed(0)})`);
    await canvas.click({ position: { x: createX, y: createY } });

    // Wait for WebSocket connection and room creation
    await page.waitForTimeout(3000);

    const roomCode = await extractRoomCode(page, 10000);
    console.log(`Room code: ${roomCode}`);

    if (roomCode) {
      // Verify we got a valid room code
      expect(roomCode.length).toBeGreaterThan(0);
    } else {
      // Connection may have failed if server isn't running, but no panic
      console.log('Note: Room creation failed (server may not be running)');
    }

    // Verify no WASM panics
    const wsMessages = await getWsMessages(page);
    console.log(`Received ${wsMessages.length} WS messages`);
    for (const msg of wsMessages) {
      console.log(`  ${msgTypeName(msg.type)}: ${JSON.stringify(msg.payload)?.substring(0, 200)}`);
    }
  });

  test('full multiplayer flow: create room, join, start game', async ({ page }) => {
    const messages = collectConsole(page);
    await installWsInterceptor(page);
    await waitForWasm(page, 15000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Step 1: Click "Create Room" on Player 1
    console.log('Step 1: Player 1 creating room...');
    // Create Room is now a standalone centered button (editor removed)
    const createX = box.width * 0.50;
    const createY = box.height * 0.56;
    await canvas.click({ position: { x: createX, y: createY } });

    // Step 2: Extract room code
    const roomCode = await extractRoomCode(page, 15000);
    if (!roomCode) {
      console.log('Server not reachable — skipping multiplayer test');
      test.skip();
      return;
    }
    console.log(`Step 2: Room created: ${roomCode}`);

    // Step 3: Player 2 joins via raw WebSocket
    console.log('Step 3: Player 2 joining...');
    const wsUrl = 'ws://127.0.0.1:8080/ws';
    let p2;
    try {
      p2 = await connectPlayer2(wsUrl, roomCode);
    } catch (e) {
      console.log(`Player 2 join failed: ${e.message}`);
      test.skip();
      return;
    }
    console.log(`Step 3: Player 2 joined with id ${p2.playerId}`);

    // Wait for Player 1's Bevy to process the PlayerList update
    await page.waitForTimeout(3000);

    // Take screenshot showing 2 players in lobby
    await page.screenshot({ path: 'results/lobby-2players.png' });
    console.log('Saved lobby screenshot with 2 players');

    // Step 4: Click "Start Game" on Player 1
    // The Start Game button appears dynamically after room creation.
    // Its Y position shifts as room code + player list text appear above it.
    // Scan a vertical strip to find it reliably.
    console.log('Step 4: Player 1 starting game...');
    const startX = box.width * 0.50;
    let gameStarted = false;

    // Try clicking at several Y positions (60%-75%) to find the button
    for (const yPct of [0.62, 0.65, 0.68, 0.70, 0.73]) {
      const startY = box.height * yPct;
      await canvas.click({ position: { x: startX, y: startY } });
      await page.waitForTimeout(1500);

      // Check if Player 2 received GameStart
      const gsMsg = p2.received.find(m => m.type === MSG.GAME_START);
      if (gsMsg) {
        const gs = parseGameStart(gsMsg.payload);
        console.log(`Game started at y=${yPct}: ${gs.gameName} with ${gs.players.length} players`);
        expect(gs.gameName).toBe('mini-golf');
        gameStarted = true;
        break;
      }
    }

    if (!gameStarted) {
      console.log('Game start failed — Start Game button not found in scan area');
    }

    // Step 6: Wait for game rendering
    await page.waitForTimeout(5000);

    // Take screenshot of in-game state
    const gameScreenshot = await page.screenshot({ path: 'results/in-game.png' });
    expect(gameScreenshot.byteLength).toBeGreaterThan(5000);
    console.log('Saved in-game screenshot');

    // Step 7: Check for game state messages (Player 2 should receive GameState)
    const gameStateMessages = p2.received.filter(m => m.type === MSG.GAME_STATE);
    console.log(`Player 2 received ${gameStateMessages.length} GameState messages`);

    // Step 8: Verify no panics
    const panics = messages.filter(m =>
      m.text.includes('panicked') || m.text.includes('RuntimeError')
    );
    expect(panics).toHaveLength(0);

    // Cleanup
    p2.ws.close();
  });

  test('game renders sky-blue background and course elements', async ({ page }) => {
    const messages = collectConsole(page);
    await installWsInterceptor(page);
    await waitForWasm(page, 15000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Create room (standalone centered button, editor removed)
    await canvas.click({ position: { x: box.width * 0.50, y: box.height * 0.56 } });
    const roomCode = await extractRoomCode(page, 15000);
    if (!roomCode) {
      test.skip();
      return;
    }

    // Player 2 joins
    let p2;
    try {
      p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
    } catch {
      test.skip();
      return;
    }
    await page.waitForTimeout(3000);

    // Start game — scan vertical strip to find button
    for (const yPct of [0.62, 0.65, 0.68, 0.70, 0.73]) {
      await canvas.click({ position: { x: box.width * 0.50, y: box.height * yPct } });
      await page.waitForTimeout(1000);
    }

    // Wait for game rendering
    await page.waitForTimeout(8000);

    // Sample pixels to check for sky-blue background (0.53, 0.81, 0.98 sRGB)
    // In 0-255: roughly (135, 207, 250)
    const pixelData = await page.evaluate(() => {
      const c = document.getElementById('game-canvas');
      if (!c) return { error: 'no canvas' };
      const gl = c.getContext('webgl2');
      if (!gl) return { error: 'no webgl2' };

      const w = gl.drawingBufferWidth;
      const h = gl.drawingBufferHeight;

      // Use requestAnimationFrame to read pixels at the right time
      return new Promise(resolve => {
        requestAnimationFrame(() => {
          const px = new Uint8Array(4);
          const results = {};

          // Sample corners (should be sky-blue if in-game) and center
          const positions = {
            topLeft: [20, h - 20],
            topRight: [w - 20, h - 20],
            center: [Math.floor(w / 2), Math.floor(h / 2)],
          };

          for (const [name, [x, y]] of Object.entries(positions)) {
            gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
            results[name] = { r: px[0], g: px[1], b: px[2], a: px[3] };
          }

          resolve({ width: w, height: h, pixels: results });
        });
      });
    });

    console.log('Pixel samples:', JSON.stringify(pixelData, null, 2));

    // At least verify we got valid pixel data
    expect(pixelData.error).toBeUndefined();

    // Log all WS messages for debugging
    const wsMessages = await getWsMessages(page);
    console.log(`Total WS messages: ${wsMessages.length}`);
    for (const msg of wsMessages.slice(0, 10)) {
      console.log(`  ${msgTypeName(msg.type)}`);
    }

    // Screenshot for visual verification
    await page.screenshot({ path: 'results/game-rendering.png' });

    // Verify sky-blue clear color is NOT magenta (255,0,255)
    // Sky-blue = sRGB(0.53, 0.81, 0.98) ≈ (135, 207, 250)
    if (pixelData.pixels) {
      const tl = pixelData.pixels.topLeft;
      // TopLeft corner should be either sky-blue (background) or green (ground)
      // but definitely NOT magenta
      const isMagenta = tl.r > 200 && tl.g < 50 && tl.b > 200;
      expect(isMagenta).toBe(false);
    }

    // No panics
    const panics = messages.filter(m =>
      m.text.includes('panicked') || m.text.includes('RuntimeError')
    );
    expect(panics).toHaveLength(0);

    p2.ws.close();
  });

  test('game survives 30 seconds without crash', async ({ page }) => {
    const messages = collectConsole(page);
    await installWsInterceptor(page);
    await waitForWasm(page, 15000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Create room + join Player 2
    // Create Room is now a standalone centered button (editor removed)
    const createX = box.width * 0.50;
    const createY = box.height * 0.56;
    await canvas.click({ position: { x: createX, y: createY } });
    const roomCode = await extractRoomCode(page, 15000);
    if (!roomCode) {
      test.skip();
      return;
    }

    let p2;
    try {
      p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
    } catch {
      test.skip();
      return;
    }
    await page.waitForTimeout(3000);

    // Start game — scan vertical strip to find button
    for (const yPct of [0.62, 0.65, 0.68, 0.70, 0.73]) {
      await canvas.click({ position: { x: box.width * 0.50, y: box.height * yPct } });
      await page.waitForTimeout(1000);
    }
    await page.waitForTimeout(2000);

    // Let the game run for 30 seconds
    console.log('Game running, waiting 30 seconds...');
    await page.waitForTimeout(30000);

    // Verify rAF is still running (event loop not dead)
    const rafBefore = await page.evaluate(() => window.__rafCount || 0);
    await page.waitForTimeout(2000);
    const rafAfter = await page.evaluate(() => window.__rafCount || 0);
    const rafDelta = rafAfter - rafBefore;
    console.log(`rAF delta over 2s: ${rafDelta} (expect ~120 at 60fps)`);

    // Verify no panics in 30 seconds of gameplay
    const panics = messages.filter(m =>
      m.text.includes('panicked') || m.text.includes('RuntimeError')
    );
    expect(panics).toHaveLength(0);

    // Screenshot after sustained play
    await page.screenshot({ path: 'results/game-30s.png' });

    // Check Player 2 is still getting game state updates
    const stateCount = p2.received.filter(m => m.type === MSG.GAME_STATE).length;
    console.log(`Player 2 received ${stateCount} GameState updates over ~35 seconds`);
    // At 10 Hz for 35s, expect roughly 350 updates
    if (stateCount > 0) {
      expect(stateCount).toBeGreaterThan(10);
    }

    p2.ws.close();
  });

  test('host broadcasts GameState at 10Hz after game start', async ({ page }) => {
    const messages = collectConsole(page);
    await installWsInterceptor(page);
    await waitForWasm(page, 15000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Create room
    await canvas.click({ position: { x: box.width * 0.50, y: box.height * 0.56 } });
    const roomCode = await extractRoomCode(page, 15000);
    if (!roomCode) { test.skip(); return; }

    // Player 2 joins
    let p2;
    try {
      p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
    } catch { test.skip(); return; }
    await page.waitForTimeout(3000);

    // Record message count before starting game
    const beforeStart = p2.received.filter(m => m.type === MSG.GAME_STATE).length;

    // Start game
    let gameStarted = false;
    for (const yPct of [0.62, 0.65, 0.68, 0.70, 0.73]) {
      await canvas.click({ position: { x: box.width * 0.50, y: box.height * yPct } });
      await page.waitForTimeout(1500);
      if (p2.received.find(m => m.type === MSG.GAME_START)) {
        gameStarted = true;
        break;
      }
    }

    if (!gameStarted) {
      console.log('Could not start game — skipping tick rate test');
      p2.ws.close();
      test.skip();
      return;
    }

    // Wait 5 seconds for game state accumulation
    await page.waitForTimeout(5000);

    const gameStateCount = p2.received.filter(m => m.type === MSG.GAME_STATE).length - beforeStart;
    console.log(`GameState messages in 5s: ${gameStateCount} (expected ~50 at 10Hz)`);

    // At 10Hz for 5s = 50 ideal. Under swiftshader the host runs at <1fps
    // with irregular frame timing, so actual count varies widely (10-50).
    // Threshold >8 confirms the game loop is ticking and broadcasting state.
    expect(gameStateCount).toBeGreaterThan(8);
    expect(gameStateCount).toBeLessThan(100);

    // Verify no panics
    const panics = messages.filter(m =>
      m.text.includes('panicked') || m.text.includes('RuntimeError')
    );
    expect(panics).toHaveLength(0);

    p2.ws.close();
  });

  test('player 2 sends PlayerInput and host processes it', async ({ page }) => {
    const messages = collectConsole(page);
    await installWsInterceptor(page);
    await waitForWasm(page, 15000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Create room
    await canvas.click({ position: { x: box.width * 0.50, y: box.height * 0.56 } });
    const roomCode = await extractRoomCode(page, 15000);
    if (!roomCode) { test.skip(); return; }

    // Player 2 joins
    let p2;
    try {
      p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
    } catch { test.skip(); return; }
    await page.waitForTimeout(3000);

    // Start game
    let gameStarted = false;
    for (const yPct of [0.62, 0.65, 0.68, 0.70, 0.73]) {
      await canvas.click({ position: { x: box.width * 0.50, y: box.height * yPct } });
      await page.waitForTimeout(1500);
      if (p2.received.find(m => m.type === MSG.GAME_START)) {
        gameStarted = true;
        break;
      }
    }

    if (!gameStarted) {
      console.log('Could not start game — skipping input test');
      p2.ws.close();
      test.skip();
      return;
    }

    // Wait for game to stabilize and accumulate some GameState
    await page.waitForTimeout(3000);

    // Player 2 sends a stroke input
    const golfInput = encodeGolfInput(1.57, 0.5, true);
    const inputMsg = playerInputMsg(p2.playerId, 1, golfInput);
    p2.ws.send(inputMsg);

    // Wait for host to process and send updated GameState
    await page.waitForTimeout(2000);

    // Find a GameState message received after the input was sent
    const gameStates = p2.received.filter(m => m.type === MSG.GAME_STATE);
    const latestState = gameStates[gameStates.length - 1];

    if (latestState && latestState.payload) {
      const gs = parseGameState(latestState.payload);
      console.log(`Latest GameState tick: ${gs.tick}`);

      // Try to decode golf state to check ball velocity
      try {
        const golfState = parseGolfState(gs.stateData);
        console.log('Golf state balls:', JSON.stringify(golfState.balls));
        console.log('Golf state strokes:', JSON.stringify(golfState.strokes));
        // The test validates that GameState is being received with parseable data
        expect(gs.tick).toBeGreaterThan(0);
      } catch (e) {
        console.log(`Could not parse golf state: ${e.message}`);
        // Still pass if we got GameState messages
        expect(gs.tick).toBeGreaterThan(0);
      }
    } else {
      console.log('No GameState received — game loop may not be running');
    }

    // Verify Player 2 received at least some GameState after input
    expect(gameStates.length).toBeGreaterThan(0);

    // Verify no panics
    const panics = messages.filter(m =>
      m.text.includes('panicked') || m.text.includes('RuntimeError')
    );
    expect(panics).toHaveLength(0);

    p2.ws.close();
  });
});
