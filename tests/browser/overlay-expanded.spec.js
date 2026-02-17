/**
 * Expanded Overlay & Toast Browser Tests
 *
 * Tests toast rendering, claiming, dismissal, ticker bar, dashboard panel,
 * and overlay behavior during active gameplay.
 *
 * These tests inject overlay state directly via _breakpointUpdate() to test
 * the UI layer independently of the full WASM pipeline.
 *
 * Requires the breakpoint server running on http://127.0.0.1:8080
 */
import { test, expect } from '@playwright/test';
import { collectConsole, panics } from './helpers/shared.js';

test.describe.configure({ timeout: 120_000 });

/**
 * Inject a mock state update with overlay data.
 */
async function injectOverlayState(page, overlay) {
  await page.evaluate((ov) => {
    if (window._breakpointUpdate) {
      window._breakpointUpdate({
        appState: 'Lobby',
        lobby: {
          playerName: 'TestPlayer',
          connected: false,
          roomCode: null,
          players: [],
          isLeader: false,
          selectedGame: 'mini-golf',
          statusMessage: '',
          errorMessage: '',
        },
        overlay: ov,
        muted: false,
      });
    }
  }, overlay);
}

test.describe('Toast Rendering', () => {
  test('toast appears when overlay state has toasts', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    // Inject a toast via overlay state
    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 1,
      pendingActions: 0,
      toasts: [{
        id: 'test-toast-1',
        title: 'CI Build Failed',
        source: 'GitHub Actions',
        actor: 'dependabot',
        priority: 'high',
        claimedBy: null,
      }],
    });

    await page.waitForTimeout(500);

    // Toast should be visible
    const toast = page.locator('[data-testid="toast-test-toast-1"]');
    await expect(toast).toBeAttached();

    // Check toast content
    const title = toast.locator('[data-testid="toast-title"]');
    await expect(title).toContainText('CI Build Failed');

    const meta = toast.locator('[data-testid="toast-meta"]');
    await expect(meta).toContainText('GitHub Actions');
    await expect(meta).toContainText('dependabot');

    // Claim button should be present
    const claimBtn = toast.locator('[data-testid="toast-claim-btn"]');
    await expect(claimBtn).toBeAttached();
  });

  test('toast shows claimed state', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    // Inject a pre-claimed toast
    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 0,
      pendingActions: 0,
      toasts: [{
        id: 'claimed-toast-1',
        title: 'Deploy Complete',
        source: 'CI',
        actor: null,
        priority: 'low',
        claimedBy: 'alice',
      }],
    });

    await page.waitForTimeout(500);

    const toast = page.locator('[data-testid="toast-claimed-toast-1"]');
    await expect(toast).toBeAttached();

    // Should show claimed by alice, not a claim button
    const claimed = toast.locator('[data-testid="toast-claimed"]');
    await expect(claimed).toContainText('Claimed by alice');

    const claimBtn = toast.locator('[data-testid="toast-claim-btn"]');
    await expect(claimBtn).not.toBeAttached();
  });

  test('toast updates from unclaimed to claimed', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    // First: inject unclaimed toast
    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 1,
      pendingActions: 0,
      toasts: [{
        id: 'update-toast-1',
        title: 'New PR',
        source: 'GitHub',
        actor: 'bot',
        priority: 'medium',
        claimedBy: null,
      }],
    });
    await page.waitForTimeout(500);

    // Claim button should exist initially
    const toast = page.locator('[data-testid="toast-update-toast-1"]');
    let claimBtn = toast.locator('[data-testid="toast-claim-btn"]');
    await expect(claimBtn).toBeAttached();

    // Now update with claimed state
    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 0,
      pendingActions: 0,
      toasts: [{
        id: 'update-toast-1',
        title: 'New PR',
        source: 'GitHub',
        actor: 'bot',
        priority: 'medium',
        claimedBy: 'bob',
      }],
    });
    await page.waitForTimeout(500);

    // Now should show claimed
    const claimed = toast.locator('.toast-claimed');
    await expect(claimed).toContainText('Claimed by bob');
  });

  test('toast is removed when dismissed from state', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    // Add toast
    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 1,
      pendingActions: 0,
      toasts: [{
        id: 'dismiss-toast-1',
        title: 'Temporary Alert',
        source: 'Test',
        actor: null,
        priority: 'low',
        claimedBy: null,
      }],
    });
    await page.waitForTimeout(500);

    const toast = page.locator('[data-testid="toast-dismiss-toast-1"]');
    await expect(toast).toBeAttached();

    // Remove toast from state (simulate dismissal)
    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 0,
      pendingActions: 0,
      toasts: [],
    });
    await page.waitForTimeout(500);

    await expect(toast).not.toBeAttached();
  });

  test('multiple toasts stack correctly', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 3,
      pendingActions: 0,
      toasts: [
        { id: 'stack-1', title: 'First', source: 'A', actor: null, priority: 'low', claimedBy: null },
        { id: 'stack-2', title: 'Second', source: 'B', actor: null, priority: 'medium', claimedBy: null },
        { id: 'stack-3', title: 'Third', source: 'C', actor: null, priority: 'high', claimedBy: null },
      ],
    });
    await page.waitForTimeout(500);

    // All three should be visible
    const container = page.locator('[data-testid="toast-container"]');
    const toasts = container.locator('.toast');
    await expect(toasts).toHaveCount(3);

    // Check priority classes
    const first = page.locator('[data-testid="toast-stack-1"]');
    await expect(first).toHaveClass(/priority-low/);
    const third = page.locator('[data-testid="toast-stack-3"]');
    await expect(third).toHaveClass(/priority-high/);
  });
});

test.describe('Ticker Bar', () => {
  test('ticker bar shows text when present in state', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    await injectOverlayState(page, {
      tickerText: 'Build #42 deploying to production',
      unreadCount: 0,
      pendingActions: 0,
      toasts: [],
    });
    await page.waitForTimeout(500);

    const tickerBar = page.locator('[data-testid="ticker-bar"]');
    const isHidden = await tickerBar.evaluate(el => el.classList.contains('hidden'));
    expect(isHidden).toBe(false);

    const tickerText = page.locator('[data-testid="ticker-text"]');
    await expect(tickerText).toContainText('Build #42 deploying to production');
  });

  test('ticker bar hides when text is null', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    // Show ticker
    await injectOverlayState(page, {
      tickerText: 'Active deployment',
      unreadCount: 0,
      pendingActions: 0,
      toasts: [],
    });
    await page.waitForTimeout(500);

    // Hide ticker
    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 0,
      pendingActions: 0,
      toasts: [],
    });
    await page.waitForTimeout(500);

    const tickerBar = page.locator('[data-testid="ticker-bar"]');
    const isHidden = await tickerBar.evaluate(el => el.classList.contains('hidden'));
    expect(isHidden).toBe(true);
  });
});

test.describe('Dashboard Badge', () => {
  test('dashboard badge shows unread count', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 5,
      pendingActions: 0,
      toasts: [],
    });
    await page.waitForTimeout(500);

    const btn = page.locator('[data-testid="btn-dashboard"]');
    const isHidden = await btn.evaluate(el => el.classList.contains('hidden'));
    expect(isHidden).toBe(false);

    const badge = page.locator('[data-testid="badge-count"]');
    await expect(badge).toContainText('5');
  });

  test('dashboard button shows with pending actions but no badge', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 0,
      pendingActions: 3,
      toasts: [],
    });
    await page.waitForTimeout(500);

    const btn = page.locator('[data-testid="btn-dashboard"]');
    const btnHidden = await btn.evaluate(el => el.classList.contains('hidden'));
    expect(btnHidden).toBe(false);

    const badge = page.locator('[data-testid="badge-count"]');
    const badgeHidden = await badge.evaluate(el => el.classList.contains('hidden'));
    expect(badgeHidden).toBe(true);
  });

  test('dashboard button hides when no unread and no pending', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    await injectOverlayState(page, {
      tickerText: null,
      unreadCount: 0,
      pendingActions: 0,
      toasts: [],
    });
    await page.waitForTimeout(500);

    const btn = page.locator('[data-testid="btn-dashboard"]');
    const isHidden = await btn.evaluate(el => el.classList.contains('hidden'));
    expect(isHidden).toBe(true);
  });
});
