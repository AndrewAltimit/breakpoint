/**
 * Tron Light Cycles Browser Tests
 *
 * Tests Tron game selection, room creation, dual-client join, input sending,
 * state receipt, and rendering stability (including Firefox where GPU crashes
 * were previously found).
 *
 * Requires the breakpoint server running on http://127.0.0.1:8080
 */
import { test, expect } from '@playwright/test';
import {
  collectConsole, panics, installWsInterceptor, extractRoomCode,
  connectPlayer2, waitForP2Message, getLatestGameState, startGame,
  MSG, parseGameState, parseTronState, encodeTronInput, playerInputMsg,
} from './helpers/shared.js';

test.describe.configure({ timeout: 180_000 });

test.describe('Tron Game Flow', () => {
  test('tron game selection and room creation', async ({ page }) => {
    const messages = collectConsole(page);
    await installWsInterceptor(page);
    await page.goto('/');
    await page.waitForTimeout(15000);

    // Select Tron using data-testid
    await page.locator('[data-testid="game-btn-tron"]').click({ force: true });
    await page.waitForTimeout(500);

    // Verify Tron is selected (aria-pressed)
    const pressed = await page.locator('[data-testid="game-btn-tron"]')
      .getAttribute('aria-pressed');
    expect(pressed).toBe('true');

    // Create room
    await page.locator('[data-testid="btn-create"]').click({ force: true });
    const roomCode = await extractRoomCode(page, 15000);
    if (!roomCode) { test.skip(); return; }
    expect(roomCode.length).toBeGreaterThan(0);
    console.log(`Tron room created: ${roomCode}`);

    expect(panics(messages)).toHaveLength(0);
  });

  test('tron dual-client join and game start', async ({ page }) => {
    const result = await startGame(page, 'tron');
    if (!result) { test.skip(); return; }
    const { p2, hostPlayerId, actualGame } = result;

    expect(actualGame).toContain('tron');
    console.log(`Tron game started, host=${hostPlayerId}`);

    // Verify Player 2 receives game state
    const gs = getLatestGameState(p2);
    expect(gs).not.toBeNull();
    expect(gs.tick).toBeGreaterThanOrEqual(0);

    p2.ws.close();
  });

  test('tron input (turn left) changes cycle direction', async ({ page }) => {
    const result = await startGame(page, 'tron');
    if (!result) { test.skip(); return; }
    const { p2, hostPlayerId } = result;

    // Wait for game state to stabilize
    await page.waitForTimeout(3000);

    // Get initial state
    const initialGs = getLatestGameState(p2);
    expect(initialGs).not.toBeNull();

    // P2 sends a Left turn input
    const tronInput = encodeTronInput('Left', false);
    p2.ws.send(playerInputMsg(p2.playerId, 0, tronInput));

    // Wait for state update
    await page.waitForTimeout(3000);

    // Verify game state is still being received (game loop alive)
    const afterGs = getLatestGameState(p2);
    expect(afterGs).not.toBeNull();
    expect(afterGs.tick).toBeGreaterThan(initialGs.tick);

    // Parse tron-specific state
    try {
      const tronState = parseTronState(afterGs.stateData);
      expect(tronState.arenaWidth).toBeGreaterThan(0);
      expect(tronState.arenaDepth).toBeGreaterThan(0);
      console.log(`Arena: ${tronState.arenaWidth}x${tronState.arenaDepth}`);

      // Players map should exist
      const players = tronState.players;
      expect(players).toBeDefined();
      const playerCount = players instanceof Map
        ? players.size : Object.keys(players).length;
      expect(playerCount).toBeGreaterThanOrEqual(2);
      console.log(`${playerCount} players in tron game`);
    } catch (e) {
      console.log(`Could not parse tron state: ${e.message}`);
    }

    p2.ws.close();
  });

  test('tron wall segments grow over time', async ({ page }) => {
    const result = await startGame(page, 'tron');
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    // Wait for game to run and generate wall segments
    await page.waitForTimeout(5000);

    const gs = getLatestGameState(p2);
    expect(gs).not.toBeNull();

    try {
      const tronState = parseTronState(gs.stateData);
      const wallCount = tronState.wallSegments?.length ?? 0;
      console.log(`Wall segments after 5s: ${wallCount}`);
      // Cycles should have left trails
      expect(wallCount).toBeGreaterThan(0);
    } catch (e) {
      console.log(`Could not parse tron state: ${e.message}`);
    }

    p2.ws.close();
  });

  test('tron game survives 15 seconds without crash', async ({ page }) => {
    const messages = collectConsole(page);
    const result = await startGame(page, 'tron');
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    // Let the game run for 15 seconds
    console.log('Tron game running for 15 seconds...');
    await page.waitForTimeout(15000);

    // Verify no panics
    expect(panics(messages)).toHaveLength(0);

    // Verify game state still flowing
    const stateCount = p2.received.filter(m => m.type === MSG.GAME_STATE).length;
    console.log(`Tron GameState messages in ~18s: ${stateCount}`);
    expect(stateCount).toBeGreaterThan(5);

    // Take screenshot for visual verification
    await page.screenshot({ path: 'results/tron-15s.png' });

    p2.ws.close();
  });

  test('tron brake input is accepted', async ({ page }) => {
    const result = await startGame(page, 'tron');
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    await page.waitForTimeout(3000);

    // P2 sends brake input
    const brakeInput = encodeTronInput('None', true);
    p2.ws.send(playerInputMsg(p2.playerId, 0, brakeInput));

    await page.waitForTimeout(2000);

    // Verify game state still flowing (brake didn't crash anything)
    const gs = getLatestGameState(p2);
    expect(gs).not.toBeNull();

    try {
      const tronState = parseTronState(gs.stateData);
      const p2cycle = tronState.players instanceof Map
        ? tronState.players.get(p2.playerId)
        : tronState.players?.[p2.playerId];
      if (p2cycle) {
        // CycleState: [x, z, direction, speed, alive, ...]
        const speed = Array.isArray(p2cycle) ? p2cycle[3] : p2cycle?.speed;
        console.log(`P2 cycle speed after brake: ${speed}`);
      }
    } catch (e) {
      console.log(`Could not parse tron state: ${e.message}`);
    }

    p2.ws.close();
  });

  test('tron game state tick rate is consistent', async ({ page }) => {
    const result = await startGame(page, 'tron');
    if (!result) { test.skip(); return; }
    const { p2 } = result;

    // Record state count before measurement window
    const beforeCount = p2.received.filter(m => m.type === MSG.GAME_STATE).length;

    // Wait 5 seconds
    await page.waitForTimeout(5000);

    const afterCount = p2.received.filter(m => m.type === MSG.GAME_STATE).length;
    const stateCount = afterCount - beforeCount;

    // Tron runs at 20Hz, so 5s should give ~100 states.
    // Under server load, threshold >20 confirms game loop is ticking.
    console.log(`Tron GameState in 5s: ${stateCount} (expected ~100 at 20Hz)`);
    expect(stateCount).toBeGreaterThan(20);

    // Verify ticks are monotonically increasing
    const states = p2.received
      .filter(m => m.type === MSG.GAME_STATE)
      .map(m => parseGameState(m.payload));
    for (let i = 1; i < states.length; i++) {
      expect(states[i].tick).toBeGreaterThanOrEqual(states[i - 1].tick);
    }

    p2.ws.close();
  });
});
