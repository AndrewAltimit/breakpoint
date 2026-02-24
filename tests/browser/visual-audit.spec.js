/**
 * Visual Audit: Platformer Castlevania Style
 *
 * Captures 6 screenshots across different gameplay moments for visual analysis.
 * Designed for the Castlevania GBA/DS restyle feedback loop:
 *   1. Playwright captures screenshots → disk
 *   2. Claude Code reads them (multimodal)
 *   3. Gemini analyzes against reference aesthetics
 *   4. Improvements get implemented
 *
 * Saves artifacts to tests/browser/results/visual-audit/
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
  parsePlatformerState,
} from './helpers/protocol.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const OUT_DIR = path.join(__dirname, 'results', 'visual-audit');

fs.mkdirSync(OUT_DIR, { recursive: true });

test.describe.configure({ timeout: 180_000 });

// ============================================================================
// Helpers (same patterns as visual-debug.spec.js)
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
      ws.send(joinRoomMsg(roomCode, 'AuditBot', [200, 100, 50]));
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

async function launchPlatformer(page) {
  await installWsInterceptor(page);
  await page.goto('/');
  // Wait for WASM init (swiftshader is slow)
  await page.waitForTimeout(15000);
  const canvas = page.locator('#game-canvas');
  await expect(canvas).toBeAttached({ timeout: 5000 });

  // Select platform-racer via HTML data-testid button
  const gameBtn = page.locator('[data-testid="game-btn-platform-racer"]');
  await gameBtn.click({ force: true });
  await page.waitForTimeout(500);

  // Create Room via HTML button
  const createBtn = page.locator('[data-testid="btn-create"]');
  await createBtn.click({ force: true });
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

  // Start game via HTML button
  try {
    const startBtn = page.locator('[data-testid="btn-start"]');
    await startBtn.click({ force: true });
  } catch {
    await page.waitForTimeout(2000);
    try {
      const startBtn = page.locator('[data-testid="btn-start"]');
      await startBtn.click({ force: true });
    } catch { return null; }
  }

  // Wait for GAME_START message
  let gameStarted = false;
  const deadline = Date.now() + 10000;
  while (Date.now() < deadline) {
    if (p2.received.find(m => m.type === MSG.GAME_START)) {
      gameStarted = true;
      break;
    }
    await page.waitForTimeout(500);
  }
  if (!gameStarted) return null;

  const gsMsg = p2.received.find(m => m.type === MSG.GAME_START);
  const gs = parseGameStart(gsMsg.payload);

  // Wait for game to initialize and render
  await page.waitForTimeout(3000);
  const box = await canvas.boundingBox();
  return { canvas, box, p2, hostPlayerId: gs.hostId, actualGame: gs.gameName };
}

/**
 * Sample WebGL pixels at a grid across the canvas.
 */
async function samplePixels(page, gridSize = 10) {
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
 * Compute a color histogram from WebGL readPixels (full canvas).
 * Buckets RGB into 4-bit bins (16 levels per channel → 4096 buckets).
 */
async function computeColorHistogram(page) {
  return page.evaluate(() => {
    const c = document.getElementById('game-canvas');
    if (!c) return { error: 'no canvas' };
    const gl = c.getContext('webgl2');
    if (!gl) return { error: 'no webgl2' };
    const w = gl.drawingBufferWidth;
    const h = gl.drawingBufferHeight;
    return new Promise(resolve => {
      requestAnimationFrame(() => {
        const pixels = new Uint8Array(w * h * 4);
        gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        const totalPixels = w * h;

        // Aggregate into 16-level bins per channel
        const rHist = new Array(16).fill(0);
        const gHist = new Array(16).fill(0);
        const bHist = new Array(16).fill(0);

        // Track dominant colors (quantize to 4-bit)
        const colorCounts = {};
        let darkPixels = 0;
        let brightPixels = 0;

        for (let i = 0; i < pixels.length; i += 4) {
          const r = pixels[i];
          const g = pixels[i + 1];
          const b = pixels[i + 2];

          const rBin = r >> 4;
          const gBin = g >> 4;
          const bBin = b >> 4;

          rHist[rBin]++;
          gHist[gBin]++;
          bHist[bBin]++;

          // Track luminance
          const lum = 0.299 * r + 0.587 * g + 0.114 * b;
          if (lum < 40) darkPixels++;
          if (lum > 200) brightPixels++;

          // Top colors (quantized)
          const key = `${rBin},${gBin},${bBin}`;
          colorCounts[key] = (colorCounts[key] || 0) + 1;
        }

        // Sort and take top 20 colors
        const topColors = Object.entries(colorCounts)
          .sort((a, b) => b[1] - a[1])
          .slice(0, 20)
          .map(([key, count]) => {
            const [r, g, b] = key.split(',').map(Number);
            return {
              rgb: `rgb(${r * 16 + 8},${g * 16 + 8},${b * 16 + 8})`,
              bin: key,
              count,
              pct: ((count / totalPixels) * 100).toFixed(2),
            };
          });

        resolve({
          totalPixels,
          width: w,
          height: h,
          darkPixelPct: ((darkPixels / totalPixels) * 100).toFixed(2),
          brightPixelPct: ((brightPixels / totalPixels) * 100).toFixed(2),
          rHistogram: rHist,
          gHistogram: gHist,
          bHistogram: bHist,
          topColors,
        });
      });
    });
  });
}

/**
 * Capture canvas content as PNG via readPixels (bypasses preserveDrawingBuffer issue).
 * Uses readPixels inside requestAnimationFrame to get the actual rendered frame,
 * then reconstructs the image data.
 */
async function captureCanvas(page, filename) {
  const pngDataUrl = await page.evaluate(() => {
    const c = document.getElementById('game-canvas');
    if (!c) return null;
    const gl = c.getContext('webgl2');
    if (!gl) return null;
    const w = gl.drawingBufferWidth;
    const h = gl.drawingBufferHeight;
    return new Promise(resolve => {
      requestAnimationFrame(() => {
        // Read pixels from WebGL framebuffer
        const pixels = new Uint8Array(w * h * 4);
        gl.readPixels(0, 0, w, h, gl.RGBA, gl.UNSIGNED_BYTE, pixels);
        // WebGL readPixels is bottom-up, flip to top-down
        const flipped = new Uint8Array(w * h * 4);
        for (let row = 0; row < h; row++) {
          const srcOff = row * w * 4;
          const dstOff = (h - 1 - row) * w * 4;
          flipped.set(pixels.subarray(srcOff, srcOff + w * 4), dstOff);
        }
        // Write to a 2D canvas for PNG export
        const c2 = document.createElement('canvas');
        c2.width = w;
        c2.height = h;
        const ctx = c2.getContext('2d');
        const imgData = ctx.createImageData(w, h);
        imgData.data.set(flipped);
        ctx.putImageData(imgData, 0, 0);
        resolve(c2.toDataURL('image/png'));
      });
    });
  });
  if (pngDataUrl) {
    const base64 = pngDataUrl.replace(/^data:image\/png;base64,/, '');
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

test.describe('Visual Audit: Platformer Castlevania Style', () => {

  test('capture platformer scenes for visual audit', async ({ page }) => {
    const messages = collectConsole(page);
    const result = await launchPlatformer(page);
    if (!result) { test.skip(); return; }
    const { canvas, box, p2 } = result;

    // Focus canvas for keyboard input
    await canvas.click({ position: { x: box.width * 0.5, y: box.height * 0.5 } });

    const screenshots = [];
    const histograms = [];
    const pixelGrids = [];

    // --- Screenshot 1: Initial spawn room ---
    console.log('Capturing audit-01-initial: waiting 5s for initial scene...');
    await page.waitForTimeout(5000);
    const s1 = await captureCanvas(page, 'audit-01-initial.png');
    screenshots.push({ name: 'audit-01-initial', path: s1 });
    histograms.push({ name: 'audit-01-initial', data: await computeColorHistogram(page) });
    pixelGrids.push({ name: 'audit-01-initial', data: await samplePixels(page, 10) });

    // --- Screenshot 2: Movement (hold D for 3s) ---
    console.log('Capturing audit-02-movement: holding D for 3s...');
    await page.keyboard.down('KeyD');
    await page.waitForTimeout(3000);
    await page.keyboard.up('KeyD');
    await page.waitForTimeout(500);
    const s2 = await captureCanvas(page, 'audit-02-movement.png');
    screenshots.push({ name: 'audit-02-movement', path: s2 });
    histograms.push({ name: 'audit-02-movement', data: await computeColorHistogram(page) });
    pixelGrids.push({ name: 'audit-02-movement', data: await samplePixels(page, 10) });

    // --- Screenshot 3: Jump (press Space, wait 1s) ---
    console.log('Capturing audit-03-jump: pressing Space...');
    await page.keyboard.press('Space');
    await page.waitForTimeout(1000);
    const s3 = await captureCanvas(page, 'audit-03-jump.png');
    screenshots.push({ name: 'audit-03-jump', path: s3 });
    histograms.push({ name: 'audit-03-jump', data: await computeColorHistogram(page) });
    pixelGrids.push({ name: 'audit-03-jump', data: await samplePixels(page, 10) });

    // --- Screenshot 4: Exploration (hold D 8s + jump sequences) ---
    console.log('Capturing audit-04-exploration: exploring for 8s...');
    await page.keyboard.down('KeyD');
    await page.waitForTimeout(2000);
    await page.keyboard.press('Space');
    await page.waitForTimeout(2000);
    await page.keyboard.press('Space');
    await page.waitForTimeout(2000);
    await page.keyboard.press('Space');
    await page.waitForTimeout(2000);
    await page.keyboard.up('KeyD');
    await page.waitForTimeout(1000);
    const s4 = await captureCanvas(page, 'audit-04-exploration.png');
    screenshots.push({ name: 'audit-04-exploration', path: s4 });
    histograms.push({ name: 'audit-04-exploration', data: await computeColorHistogram(page) });
    pixelGrids.push({ name: 'audit-04-exploration', data: await samplePixels(page, 10) });

    // --- Screenshot 5: Dark area (continue moving, wait 3s) ---
    console.log('Capturing audit-05-dark-area: moving further + waiting...');
    await page.keyboard.down('KeyD');
    await page.waitForTimeout(3000);
    await page.keyboard.up('KeyD');
    await page.waitForTimeout(3000);
    const s5 = await captureCanvas(page, 'audit-05-dark-area.png');
    screenshots.push({ name: 'audit-05-dark-area', path: s5 });
    histograms.push({ name: 'audit-05-dark-area', data: await computeColorHistogram(page) });
    pixelGrids.push({ name: 'audit-05-dark-area', data: await samplePixels(page, 10) });

    // --- Screenshot 6: Combat (press F for attack, wait 2s) ---
    console.log('Capturing audit-06-combat: pressing F (attack)...');
    await page.keyboard.press('KeyF');
    await page.waitForTimeout(500);
    await page.keyboard.press('KeyF');
    await page.waitForTimeout(1500);
    const s6 = await captureCanvas(page, 'audit-06-combat.png');
    screenshots.push({ name: 'audit-06-combat', path: s6 });
    histograms.push({ name: 'audit-06-combat', data: await computeColorHistogram(page) });
    pixelGrids.push({ name: 'audit-06-combat', data: await samplePixels(page, 10) });

    // --- Collect game state from P2 ---
    const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
    const latestState = states.length > 0
      ? parseGameState(states[states.length - 1].payload) : null;
    let parsedState = null;
    if (latestState) {
      try { parsedState = parsePlatformerState(latestState.stateData); } catch { /* ignore */ }
    }

    // --- Collect console errors/panics ---
    const errors = messages.filter(m => m.type === 'error' || m.type === 'pageerror').map(m => m.text);
    const wasmPanics = messages.filter(m =>
      m.text.includes('panicked at') ||
      m.text.includes('BorrowMutError') ||
      m.text.includes('RuntimeError: unreachable')
    ).map(m => m.text);
    const breakpointLogs = messages.filter(m =>
      m.text.includes('BREAKPOINT:')
    ).map(m => m.text);

    // --- Save summary JSON ---
    const summary = {
      timestamp: new Date().toISOString(),
      screenshots: screenshots.map(s => ({ name: s.name, captured: !!s.path })),
      histograms,
      pixelGrids,
      gameState: {
        totalTicks: states.length,
        latestTick: latestState?.tick ?? null,
        parsedState,
      },
      console: {
        totalMessages: messages.length,
        errors,
        wasmPanics,
        breakpointLogs: breakpointLogs.slice(0, 50),
      },
    };
    saveJson('summary.json', summary);

    // --- Log results ---
    console.log(`\n=== VISUAL AUDIT SUMMARY ===`);
    console.log(`Screenshots captured: ${screenshots.filter(s => s.path).length}/6`);
    console.log(`Game ticks received: ${states.length}`);
    console.log(`Console errors: ${errors.length}`);
    console.log(`WASM panics: ${wasmPanics.length}`);
    for (const h of histograms) {
      console.log(`\n--- ${h.name} color profile ---`);
      console.log(`  Dark pixels: ${h.data.darkPixelPct}%`);
      console.log(`  Bright pixels: ${h.data.brightPixelPct}%`);
      if (h.data.topColors) {
        console.log(`  Top 5 colors:`);
        for (const c of h.data.topColors.slice(0, 5)) {
          console.log(`    ${c.rgb} — ${c.pct}%`);
        }
      }
    }

    // Assertions: screenshots captured and game is running
    expect(screenshots.filter(s => s.path).length).toBeGreaterThanOrEqual(4);
    expect(states.length).toBeGreaterThan(0);

    p2.ws.close();
  });
});
