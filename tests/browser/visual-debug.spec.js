/**
 * Visual Debug Tests
 *
 * Captures screenshots and console logs for all 3 games to diagnose
 * rendering issues (invisible player meshes, camera positioning, etc).
 *
 * Saves artifacts to tests/browser/results/visual-debug/ for inspection.
 *
 * Requires the breakpoint server running on http://127.0.0.1:8080
 */
import { test, expect } from '@playwright/test';
import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';
import WebSocket from 'ws';
import {
  MSG, decode, joinRoomMsg,
  parseJoinRoomResponse, parseGameStart, parseGameState,
  parseLaserTagState, parseGolfState, parsePlatformerState,
} from './helpers/protocol.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, 'results', 'visual-debug');

// Ensure output directory exists
fs.mkdirSync(OUT_DIR, { recursive: true });

test.describe.configure({ timeout: 180_000 });

// ============================================================================
// Helpers
// ============================================================================

function collectConsole(page) {
  const messages = [];
  page.on('console', msg => {
    messages.push({ type: msg.type(), text: msg.text(), ts: Date.now() });
  });
  page.on('pageerror', err => {
    messages.push({ type: 'pageerror', text: err.message, ts: Date.now() });
  });
  return messages;
}

async function installWsInterceptor(page) {
  await page.addInitScript(() => {
    window.__wsMessages = [];
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
    ws.on('error', (err) => reject(new Error(`P2 WS error: ${err.message}`)));
    setTimeout(() => reject(new Error('P2 join timed out')), 10000);
  });
}

/**
 * Start a game with P2 observer, returning all the handles needed for testing.
 * gameName: 'mini-golf' | 'platform-racer' | 'laser-tag'
 */
async function launchGame(page, gameName) {
  await installWsInterceptor(page);
  await page.goto('/');
  await page.waitForTimeout(15000);
  const canvas = page.locator('#game-canvas');
  await expect(canvas).toBeAttached({ timeout: 5000 });
  const box = await canvas.boundingBox();

  // Select game type (default is mini-golf, others need clicks)
  if (gameName === 'platform-racer') {
    for (const yPct of [0.30, 0.32, 0.34, 0.36]) {
      await canvas.click({ position: { x: box.width * 0.50, y: box.height * yPct } });
      await page.waitForTimeout(300);
    }
  } else if (gameName === 'laser-tag') {
    for (const yPct of [0.30, 0.32, 0.34, 0.36]) {
      await canvas.click({ position: { x: box.width * 0.60, y: box.height * yPct } });
      await page.waitForTimeout(300);
    }
  }

  // Create Room
  await canvas.click({ position: { x: box.width * 0.50, y: box.height * 0.56 } });
  const roomCode = await extractRoomCode(page, 15000);
  if (!roomCode) return null;

  // Connect P2
  let p2;
  try {
    p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
  } catch {
    return null;
  }
  await page.waitForTimeout(3000);

  // Start game - scan for Start button
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
  return { canvas, box, p2, hostPlayerId: gs.hostId, actualGame: gs.gameName };
}

/**
 * Sample WebGL pixels at a grid across the canvas.
 */
async function samplePixels(page, gridSize = 5) {
  return page.evaluate((gs) => {
    const c = document.getElementById('game-canvas');
    if (!c) return { error: 'no canvas' };
    const gl = c.getContext('webgl2');
    if (!gl) return { error: 'no webgl2' };
    const w = gl.drawingBufferWidth;
    const h = gl.drawingBufferHeight;
    return new Promise(resolve => {
      requestAnimationFrame(() => {
        const px = new Uint8Array(4);
        const samples = [];
        for (let gy = 0; gy < gs; gy++) {
          for (let gx = 0; gx < gs; gx++) {
            const x = Math.floor((gx + 0.5) * w / gs);
            const y = Math.floor((gy + 0.5) * h / gs);
            gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
            samples.push({
              gx, gy, x, y,
              r: px[0], g: px[1], b: px[2], a: px[3],
            });
          }
        }
        resolve({ width: w, height: h, samples });
      });
    });
  }, gridSize);
}

/**
 * Capture canvas content as a PNG data URL and save to file.
 */
async function captureCanvas(page, filename) {
  const dataUrl = await page.evaluate(() => {
    const c = document.getElementById('game-canvas');
    if (!c) return null;
    // Force a draw to ensure latest frame
    return c.toDataURL('image/png');
  });
  if (dataUrl) {
    const base64 = dataUrl.replace(/^data:image\/png;base64,/, '');
    const filePath = path.join(OUT_DIR, filename);
    fs.writeFileSync(filePath, Buffer.from(base64, 'base64'));
    return filePath;
  }
  return null;
}

function saveJson(filename, data) {
  const filePath = path.join(OUT_DIR, filename);
  fs.writeFileSync(filePath, JSON.stringify(data, null, 2));
  return filePath;
}

// ============================================================================
// Tests
// ============================================================================

test.describe('Visual Debug: Screenshot Capture', () => {

  test('laser tag - capture game rendering + diagnostics', async ({ page }) => {
    const messages = collectConsole(page);
    const result = await launchGame(page, 'laser-tag');
    if (!result) { test.skip(); return; }
    const { canvas, box, p2, hostPlayerId, actualGame } = result;

    console.log(`Game: ${actualGame}, Host PID: ${hostPlayerId}`);

    // Wait for rendering to stabilize
    await page.waitForTimeout(3000);

    // Screenshot 1: initial game state
    await page.screenshot({ path: path.join(OUT_DIR, 'lasertag-page.png') });
    await captureCanvas(page, 'lasertag-canvas.png');

    // Move mouse + press WASD for a couple seconds to trigger input
    const absX = box.x + box.width * 0.5;
    const absY = box.y + box.height * 0.5;
    await page.mouse.move(absX, absY);
    await canvas.click({ position: { x: box.width * 0.5, y: box.height * 0.5 } });
    await page.keyboard.down('KeyW');
    await page.waitForTimeout(2000);
    await page.keyboard.up('KeyW');
    await page.waitForTimeout(1000);

    // Screenshot 2: after movement attempt
    await page.screenshot({ path: path.join(OUT_DIR, 'lasertag-after-move.png') });
    await captureCanvas(page, 'lasertag-canvas-after-move.png');

    // Capture game state from P2
    const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
    const latestState = states.length > 0
      ? parseGameState(states[states.length - 1].payload) : null;
    let parsedState = null;
    if (latestState) {
      try {
        parsedState = parseLaserTagState(latestState.stateData);
      } catch (e) {
        console.log(`Failed to parse laser tag state: ${e.message}`);
      }
    }

    // Sample pixels across the canvas
    const pixels = await samplePixels(page, 8);

    // Filter BREAKPOINT diagnostic logs
    const breakpointLogs = messages.filter(m =>
      m.text.includes('BREAKPOINT:')
    ).map(m => m.text);

    // Save all diagnostics
    const diagnostics = {
      game: actualGame,
      hostPlayerId,
      totalConsoleMessages: messages.length,
      breakpointLogs,
      errors: messages.filter(m => m.type === 'error' || m.type === 'pageerror').map(m => m.text),
      gameStateCount: states.length,
      latestTick: latestState?.tick ?? null,
      parsedState: parsedState ? {
        playerCount: parsedState.players ? (parsedState.players instanceof Map
          ? parsedState.players.size : Object.keys(parsedState.players).length) : 0,
        players: parsedState.players instanceof Map
          ? Object.fromEntries(parsedState.players)
          : parsedState.players,
        roundTimer: parsedState.roundTimer ?? null,
      } : null,
      pixelSamples: pixels,
    };

    saveJson('lasertag-diagnostics.json', diagnostics);
    console.log('=== BREAKPOINT LOGS ===');
    for (const log of breakpointLogs) {
      console.log(`  ${log}`);
    }
    console.log(`=== GAME STATE ===`);
    console.log(JSON.stringify(diagnostics.parsedState, null, 2));
    console.log(`=== PIXEL SAMPLES (${pixels.samples?.length ?? 0} points) ===`);
    if (pixels.samples) {
      // Check for any non-background pixels (non-grey, non-sky-blue)
      const interesting = pixels.samples.filter(p =>
        p.r > 150 || p.g > 150 || (p.r > 100 && p.g < 50 && p.b < 50)
      );
      console.log(`  Interesting pixels (bright/colored): ${interesting.length}`);
      for (const p of interesting) {
        console.log(`    grid(${p.gx},${p.gy}) @ (${p.x},${p.y}): rgba(${p.r},${p.g},${p.b},${p.a})`);
      }
    }

    // Assertions
    expect(breakpointLogs.length).toBeGreaterThan(0);
    expect(states.length).toBeGreaterThan(0);

    p2.ws.close();
  });

  test('mini golf - capture game rendering + diagnostics', async ({ page }) => {
    const messages = collectConsole(page);
    const result = await launchGame(page, 'mini-golf');
    if (!result) { test.skip(); return; }
    const { canvas, box, p2, hostPlayerId, actualGame } = result;

    console.log(`Game: ${actualGame}, Host PID: ${hostPlayerId}`);
    await page.waitForTimeout(3000);

    // Screenshot
    await page.screenshot({ path: path.join(OUT_DIR, 'golf-page.png') });
    await captureCanvas(page, 'golf-canvas.png');

    // Try a stroke: hold mouse, release
    await page.mouse.move(box.x + box.width * 0.6, box.y + box.height * 0.3);
    await page.waitForTimeout(500);
    await page.mouse.down();
    await page.waitForTimeout(1500);
    await page.mouse.up();
    await page.waitForTimeout(3000);

    // Screenshot after stroke
    await page.screenshot({ path: path.join(OUT_DIR, 'golf-after-stroke.png') });
    await captureCanvas(page, 'golf-canvas-after-stroke.png');

    // Game state from P2
    const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
    const latestState = states.length > 0
      ? parseGameState(states[states.length - 1].payload) : null;
    let parsedState = null;
    if (latestState) {
      try { parsedState = parseGolfState(latestState.stateData); } catch { /* ignore */ }
    }

    const breakpointLogs = messages.filter(m =>
      m.text.includes('BREAKPOINT:')
    ).map(m => m.text);

    const pixels = await samplePixels(page, 8);

    saveJson('golf-diagnostics.json', {
      game: actualGame,
      hostPlayerId,
      breakpointLogs,
      errors: messages.filter(m => m.type === 'error' || m.type === 'pageerror').map(m => m.text),
      gameStateCount: states.length,
      latestTick: latestState?.tick ?? null,
      parsedState,
      pixelSamples: pixels,
    });

    console.log('=== BREAKPOINT LOGS ===');
    for (const log of breakpointLogs) console.log(`  ${log}`);
    console.log('=== GOLF STATE ===');
    console.log(JSON.stringify(parsedState, null, 2));

    expect(breakpointLogs.length).toBeGreaterThan(0);
    p2.ws.close();
  });

  test('platformer - capture game rendering + diagnostics', async ({ page }) => {
    const messages = collectConsole(page);
    const result = await launchGame(page, 'platform-racer');
    if (!result) { test.skip(); return; }
    const { canvas, box, p2, hostPlayerId, actualGame } = result;

    console.log(`Game: ${actualGame}, Host PID: ${hostPlayerId}`);
    await page.waitForTimeout(3000);

    // Screenshot
    await page.screenshot({ path: path.join(OUT_DIR, 'platformer-page.png') });
    await captureCanvas(page, 'platformer-canvas.png');

    // Try movement: press D key
    await canvas.click({ position: { x: box.width * 0.5, y: box.height * 0.5 } });
    await page.keyboard.down('KeyD');
    await page.waitForTimeout(2000);
    await page.keyboard.up('KeyD');
    await page.waitForTimeout(1000);

    // Screenshot after movement
    await page.screenshot({ path: path.join(OUT_DIR, 'platformer-after-move.png') });
    await captureCanvas(page, 'platformer-canvas-after-move.png');

    // Game state from P2
    const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
    const latestState = states.length > 0
      ? parseGameState(states[states.length - 1].payload) : null;
    let parsedState = null;
    if (latestState) {
      try { parsedState = parsePlatformerState(latestState.stateData); } catch { /* ignore */ }
    }

    const breakpointLogs = messages.filter(m =>
      m.text.includes('BREAKPOINT:')
    ).map(m => m.text);

    const pixels = await samplePixels(page, 8);

    saveJson('platformer-diagnostics.json', {
      game: actualGame,
      hostPlayerId,
      breakpointLogs,
      errors: messages.filter(m => m.type === 'error' || m.type === 'pageerror').map(m => m.text),
      gameStateCount: states.length,
      latestTick: latestState?.tick ?? null,
      parsedState,
      pixelSamples: pixels,
    });

    console.log('=== BREAKPOINT LOGS ===');
    for (const log of breakpointLogs) console.log(`  ${log}`);
    console.log('=== PLATFORMER STATE ===');
    console.log(JSON.stringify(parsedState, null, 2));

    expect(breakpointLogs.length).toBeGreaterThan(0);
    p2.ws.close();
  });
});
