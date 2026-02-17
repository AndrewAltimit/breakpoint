/**
 * Shared test helpers for Breakpoint Playwright tests.
 *
 * Consolidates the WS interceptor, room creation, player connection,
 * and game start logic that was previously duplicated across spec files.
 */
import { expect } from '@playwright/test';
import WebSocket from 'ws';
import {
  MSG, decode, joinRoomMsg,
  parseJoinRoomResponse, parseGameStart, parseGameState,
  parseGolfState, parsePlatformerState, parseLaserTagState, parseTronState,
  encodeGolfInput, encodeTronInput, playerInputMsg,
} from './protocol.js';

// Re-export protocol helpers for convenience
export {
  MSG, decode, joinRoomMsg,
  parseJoinRoomResponse, parseGameStart, parseGameState,
  parseGolfState, parsePlatformerState, parseLaserTagState, parseTronState,
  encodeGolfInput, encodeTronInput, playerInputMsg,
};

/**
 * Collect console messages and page errors.
 */
export function collectConsole(page) {
  const messages = [];
  page.on('console', msg => {
    messages.push({ type: msg.type(), text: msg.text(), ts: Date.now() });
  });
  page.on('pageerror', err => {
    messages.push({ type: 'pageerror', text: err.message, ts: Date.now() });
  });
  return messages;
}

/**
 * Filter for fatal errors (excludes known harmless warnings).
 */
export function fatalErrors(messages) {
  return messages.filter(m => {
    if (m.type !== 'error' && m.type !== 'pageerror') return false;
    const t = m.text;
    if (t.includes('favicon')) return false;
    if (t.includes('DevTools')) return false;
    if (t.includes('objects-manager')) return false;
    if (t.includes('WEBGL_debug_renderer_info')) return false;
    return true;
  });
}

/**
 * Filter for WASM panics.
 */
export function panics(messages) {
  return messages.filter(m =>
    m.text.includes('panicked at') ||
    m.text.includes('already borrowed: BorrowMutError') ||
    m.text.includes('RuntimeError: unreachable') ||
    m.text.includes('wasm-bindgen: imported JS function')
  );
}

/**
 * Install a WebSocket message interceptor on the page.
 * Must be called BEFORE navigating (via addInitScript).
 */
export async function installWsInterceptor(page) {
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
export async function extractRoomCode(page, maxWaitMs = 15000) {
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

/**
 * Connect Player 2 as a raw WebSocket and join the room.
 */
export function connectPlayer2(wsUrl, roomCode, playerName = 'TestBot') {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    const received = [];
    let playerId = null;

    ws.on('open', () => {
      ws.send(joinRoomMsg(roomCode, playerName, [200, 100, 50]));
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
export function waitForP2Message(p2, msgType, timeoutMs = 15000) {
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
 * Get the latest decoded GAME_STATE from Player 2's received messages.
 */
export function getLatestGameState(p2) {
  const states = p2.received.filter(m => m.type === MSG.GAME_STATE);
  if (states.length === 0) return null;
  const latest = states[states.length - 1];
  return parseGameState(latest.payload);
}

/**
 * Wait for WASM to initialize, then verify the lobby is visible.
 * Uses data-testid selectors for reliability.
 */
export async function waitForLobby(page, timeoutMs = 20000) {
  await page.goto('/');
  // Wait for WASM init (swiftshader is very slow)
  await page.waitForTimeout(timeoutMs);
  const canvas = page.locator('#game-canvas');
  await expect(canvas).toBeAttached({ timeout: 5000 });
}

/**
 * Poll the /health endpoint until the server responds.
 * More reliable than a fixed timeout.
 */
export async function waitForServer(baseUrl = 'http://127.0.0.1:8080', timeoutMs = 10000) {
  const start = Date.now();
  while (Date.now() - start < timeoutMs) {
    try {
      const resp = await fetch(`${baseUrl}/health`);
      if (resp.ok) return true;
    } catch { /* server not ready yet */ }
    await new Promise(r => setTimeout(r, 250));
  }
  return false;
}

/**
 * Create a room using the UI, extract room code via WS interceptor.
 * Uses data-testid selectors — clicks the HTML button, not canvas coordinates.
 */
export async function createRoom(page) {
  // Click the HTML Create Room button directly
  const btnCreate = page.locator('[data-testid="btn-create"]');
  await btnCreate.click({ force: true });

  const roomCode = await extractRoomCode(page, 15000);
  return roomCode;
}

/**
 * Select a game using data-testid selectors.
 */
export async function selectGame(page, gameName) {
  const btn = page.locator(`[data-testid="game-btn-${gameName}"]`);
  await btn.click({ force: true });
  await page.waitForTimeout(500);
}

/**
 * Click the Start Game button using data-testid.
 */
export async function clickStartGame(page) {
  const btn = page.locator('[data-testid="btn-start"]');
  await btn.click({ force: true });
}

/**
 * Full game start sequence using data-testid selectors.
 * Returns null if any step fails.
 */
export async function startGame(page, gameName = 'mini-golf') {
  await installWsInterceptor(page);
  await waitForLobby(page, 15000);

  // Select game
  if (gameName !== 'mini-golf') {
    await selectGame(page, gameName);
  }

  // Create room
  const roomCode = await createRoom(page);
  if (!roomCode) return null;

  // Connect Player 2
  let p2;
  try {
    p2 = await connectPlayer2('ws://127.0.0.1:8080/ws', roomCode);
  } catch {
    return null;
  }
  await page.waitForTimeout(3000);

  // Start game
  try {
    await clickStartGame(page);
  } catch {
    // Fallback: button might not be visible yet, retry
    await page.waitForTimeout(2000);
    try { await clickStartGame(page); } catch { return null; }
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
  const hostPlayerId = gs.hostId;
  const actualGame = gs.gameName;

  // Wait for game initialization
  await page.waitForTimeout(3000);

  return { p2, hostPlayerId, actualGame, roomCode };
}
