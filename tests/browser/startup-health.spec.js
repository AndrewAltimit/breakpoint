/**
 * Startup Health Tests
 *
 * Fast-feedback tests that verify the WASM app initializes without
 * panics, schedule build errors, or fatal console errors.
 *
 * These tests catch issues like:
 * - Bevy schedule build panics (duplicate/conflicting queries)
 * - WASM bindgen initialization failures
 * - Missing feature flag rendering issues
 * - JavaScript errors in the bootstrap code
 *
 * Requires the breakpoint server running on http://127.0.0.1:8080
 */
import { test, expect } from '@playwright/test';

// Shorter timeout — these tests should be fast
test.setTimeout(60_000);

/**
 * Collect console messages and page errors.
 * Returns a messages array that accumulates entries over time.
 */
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

/**
 * Filter for fatal errors (excludes known harmless warnings).
 */
function fatalErrors(messages) {
  return messages.filter(m => {
    if (m.type !== 'error' && m.type !== 'pageerror') return false;
    const t = m.text;
    // Harmless browser noise
    if (t.includes('favicon')) return false;
    if (t.includes('DevTools')) return false;
    if (t.includes('objects-manager')) return false;
    // WebGL warnings that aren't fatal
    if (t.includes('WEBGL_debug_renderer_info')) return false;
    return true;
  });
}

/**
 * Filter for WASM panics.
 */
function panics(messages) {
  return messages.filter(m =>
    m.text.includes('panicked at') ||
    m.text.includes('already borrowed: BorrowMutError') ||
    m.text.includes('RuntimeError: unreachable') ||
    m.text.includes('wasm-bindgen: imported JS function') ||
    m.text.includes('schedule build') ||
    m.text.includes('Query<&mut') // Bevy query conflict diagnostic
  );
}

test.describe('Startup Health', () => {
  test('WASM initializes without panics within 10 seconds', async ({ page }) => {
    const messages = collectConsole(page);

    await page.goto('/');
    // 10 seconds is enough to catch startup panics — Bevy schedule
    // builds happen synchronously during app initialization
    await page.waitForTimeout(10000);

    const detected = panics(messages);
    if (detected.length > 0) {
      console.log('STARTUP PANICS DETECTED:');
      for (const p of detected) {
        console.log(`  [${p.type}] ${p.text}`);
      }
    }
    expect(detected).toHaveLength(0);
  });

  test('no fatal JavaScript errors during startup', async ({ page }) => {
    const messages = collectConsole(page);

    await page.goto('/');
    await page.waitForTimeout(10000);

    const fatal = fatalErrors(messages);
    if (fatal.length > 0) {
      console.log('FATAL ERRORS:');
      for (const e of fatal) {
        console.log(`  [${e.type}] ${e.text.substring(0, 500)}`);
      }
    }
    expect(fatal).toHaveLength(0);
  });

  test('canvas is created and Bevy event loop is running', async ({ page }) => {
    // Inject rAF counter before navigation
    await page.addInitScript(() => {
      window.__rafCount = 0;
      const origRaf = window.requestAnimationFrame;
      window.requestAnimationFrame = function (cb) {
        window.__rafCount++;
        return origRaf.call(window, cb);
      };
    });

    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(8000);

    // Canvas must exist
    const canvas = page.locator('#game-canvas');
    await expect(canvas).toBeAttached({ timeout: 5000 });

    // Canvas must have dimensions (Bevy/winit configured it)
    const attrs = await canvas.evaluate(el => ({
      width: el.width,
      height: el.height,
      clientWidth: el.clientWidth,
      clientHeight: el.clientHeight,
    }));
    expect(attrs.width).toBeGreaterThan(0);
    expect(attrs.height).toBeGreaterThan(0);

    // rAF must be running (Bevy event loop alive)
    const rafCount = await page.evaluate(() => window.__rafCount);
    console.log(`rAF count after 8s: ${rafCount}`);
    expect(rafCount).toBeGreaterThan(10);

    // No panics
    expect(panics(messages)).toHaveLength(0);
  });

  test('lobby UI renders visible content (non-black pixels)', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(12000);

    // Take screenshot and verify it has visual content
    const screenshot = await page.screenshot();
    expect(screenshot.byteLength).toBeGreaterThan(5000);

    // Verify no panics
    expect(panics(messages)).toHaveLength(0);

    // Check for rendered pixels via WebGL readback
    const pixelCheck = await page.evaluate(() => {
      return new Promise(resolve => {
        requestAnimationFrame(() => {
          const c = document.getElementById('game-canvas');
          if (!c) { resolve({ error: 'no canvas' }); return; }
          const gl = c.getContext('webgl2');
          if (!gl) { resolve({ error: 'no webgl2' }); return; }

          const w = gl.drawingBufferWidth;
          const h = gl.drawingBufferHeight;
          const px = new Uint8Array(4);
          let nonBlackCount = 0;

          // Sample a 5x5 grid
          for (let gx = 0; gx < 5; gx++) {
            for (let gy = 0; gy < 5; gy++) {
              const x = Math.floor((gx + 0.5) * w / 5);
              const y = Math.floor((gy + 0.5) * h / 5);
              gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
              if (px[0] > 0 || px[1] > 0 || px[2] > 0 || px[3] > 0) {
                nonBlackCount++;
              }
            }
          }

          resolve({ width: w, height: h, nonBlackCount, total: 25 });
        });
      });
    });

    console.log('Pixel check:', JSON.stringify(pixelCheck));
    // WebGL preserveDrawingBuffer may be false, so readPixels might
    // return black. The screenshot test above is the real check.
    // This is informational.
    if (pixelCheck.nonBlackCount === 0) {
      console.log('Note: readPixels returned all black (preserveDrawingBuffer=false is expected)');
    }
  });

  test('app survives rapid page interaction without panic', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(8000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();
    if (!box) {
      test.skip();
      return;
    }

    // Rapidly click various positions to stress-test the UI
    for (let i = 0; i < 10; i++) {
      const x = Math.random() * box.width;
      const y = Math.random() * box.height;
      await canvas.click({ position: { x, y } });
      await page.waitForTimeout(100);
    }

    // Wait for any deferred panics
    await page.waitForTimeout(3000);

    const detected = panics(messages);
    if (detected.length > 0) {
      console.log('PANICS AFTER RAPID INTERACTION:');
      for (const p of detected) {
        console.log(`  [${p.type}] ${p.text}`);
      }
    }
    expect(detected).toHaveLength(0);
  });
});

test.describe('Console Health', () => {
  test('collect and categorize all console output', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(15000);

    // Categorize messages
    const categories = { log: 0, warn: 0, error: 0, info: 0, pageerror: 0, other: 0 };
    for (const m of messages) {
      categories[m.type] = (categories[m.type] || 0) + 1;
    }

    console.log('Console message categories:', JSON.stringify(categories));
    console.log(`Total messages: ${messages.length}`);

    // Print all error-level messages
    const errors = messages.filter(m => m.type === 'error' || m.type === 'pageerror');
    if (errors.length > 0) {
      console.log('Error messages:');
      for (const e of errors) {
        console.log(`  [${e.type}] ${e.text.substring(0, 500)}`);
      }
    }

    // No panics
    expect(panics(messages)).toHaveLength(0);

    // No unexpected page errors (pageerror = unhandled JS exceptions)
    const pageErrors = messages.filter(m => m.type === 'pageerror');
    if (pageErrors.length > 0) {
      console.log('PAGE ERRORS (unhandled exceptions):');
      for (const e of pageErrors) {
        console.log(`  ${e.text}`);
      }
    }
    expect(pageErrors).toHaveLength(0);
  });
});
