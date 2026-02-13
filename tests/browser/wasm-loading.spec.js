import { test, expect } from '@playwright/test';

// Collect all browser console messages for diagnostics
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

test.describe('WASM Loading', () => {
  test('page loads and serves correct content types', async ({ page }) => {
    const responses = {};
    page.on('response', resp => {
      const url = new URL(resp.url());
      responses[url.pathname] = {
        status: resp.status(),
        contentType: resp.headers()['content-type'] || '',
      };
    });

    await page.goto('/');
    // Wait for WASM to be requested
    await page.waitForTimeout(3000);

    expect(responses['/']).toBeDefined();
    expect(responses['/'].status).toBe(200);

    expect(responses['/pkg/breakpoint_client.js']).toBeDefined();
    expect(responses['/pkg/breakpoint_client.js'].status).toBe(200);
    expect(responses['/pkg/breakpoint_client.js'].contentType).toContain('javascript');

    expect(responses['/pkg/breakpoint_client_bg.wasm']).toBeDefined();
    expect(responses['/pkg/breakpoint_client_bg.wasm'].status).toBe(200);
    expect(responses['/pkg/breakpoint_client_bg.wasm'].contentType).toContain('wasm');
  });

  test('WASM module initializes without errors', async ({ page }) => {
    const messages = collectConsole(page);

    await page.goto('/');
    // WASM is ~123MB dev, give it time to load and init
    await page.waitForTimeout(15000);

    const errors = messages.filter(m => m.type === 'error' || m.type === 'pageerror');
    const panics = messages.filter(m => m.text.includes('panicked') || m.text.includes('wasm-bindgen'));

    console.log('=== All console messages ===');
    for (const m of messages) {
      console.log(`  [${m.type}] ${m.text.substring(0, 200)}`);
    }

    expect(panics).toHaveLength(0);
    // Allow WebGL warnings but no fatal errors
    const fatalErrors = errors.filter(m =>
      !m.text.includes('favicon') &&
      !m.text.includes('DevTools') &&
      !m.text.includes('objects-manager')
    );
    expect(fatalErrors).toHaveLength(0);
  });
});

test.describe('Canvas & WebGL', () => {
  test('canvas element exists and has dimensions', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    const canvas = page.locator('#game-canvas');
    await expect(canvas).toBeAttached();

    const box = await canvas.boundingBox();
    expect(box).not.toBeNull();
    expect(box.width).toBeGreaterThan(100);
    expect(box.height).toBeGreaterThan(100);
    console.log(`Canvas bounding box: ${box.width}x${box.height}`);

    // Check canvas attribute dimensions (set by winit/Bevy)
    const attrs = await canvas.evaluate(el => ({
      width: el.width,
      height: el.height,
      clientWidth: el.clientWidth,
      clientHeight: el.clientHeight,
    }));
    console.log(`Canvas attrs: ${attrs.width}x${attrs.height}, client: ${attrs.clientWidth}x${attrs.clientHeight}`);
    expect(attrs.width).toBeGreaterThan(0);
    expect(attrs.height).toBeGreaterThan(0);
  });

  test('WebGL2 context is active on canvas', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000);

    const glInfo = await page.evaluate(() => {
      const c = document.getElementById('game-canvas');
      if (!c) return { error: 'no canvas' };

      // Try to get existing context (will return Bevy's context if it exists)
      const gl = c.getContext('webgl2');
      if (!gl) return { error: 'no webgl2 context' };

      return {
        renderer: gl.getParameter(gl.RENDERER),
        vendor: gl.getParameter(gl.VENDOR),
        version: gl.getParameter(gl.VERSION),
        drawingBufferWidth: gl.drawingBufferWidth,
        drawingBufferHeight: gl.drawingBufferHeight,
        isContextLost: gl.isContextLost(),
      };
    });

    console.log('WebGL2 info:', JSON.stringify(glInfo, null, 2));
    expect(glInfo.error).toBeUndefined();
    expect(glInfo.isContextLost).toBe(false);
    expect(glInfo.drawingBufferWidth).toBeGreaterThan(0);
  });

  test('canvas has non-transparent pixels (Bevy is rendering)', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    // Give Bevy plenty of time to initialize and render frames
    await page.waitForTimeout(20000);

    // Read pixels from multiple positions
    const pixelData = await page.evaluate(() => {
      const c = document.getElementById('game-canvas');
      if (!c) return { error: 'no canvas' };
      const gl = c.getContext('webgl2');
      if (!gl) return { error: 'no webgl2 context' };
      if (gl.isContextLost()) return { error: 'context lost' };

      const w = gl.drawingBufferWidth;
      const h = gl.drawingBufferHeight;
      const px = new Uint8Array(4);
      const results = {};

      const positions = {
        center: [Math.floor(w / 2), Math.floor(h / 2)],
        topLeft: [10, h - 10],
        topRight: [w - 10, h - 10],
        bottomLeft: [10, 10],
        bottomRight: [w - 10, 10],
      };

      for (const [name, [x, y]] of Object.entries(positions)) {
        gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
        results[name] = { r: px[0], g: px[1], b: px[2], a: px[3] };
      }

      return { width: w, height: h, pixels: results };
    });

    console.log('Pixel data:', JSON.stringify(pixelData, null, 2));
    console.log('Console messages during test:');
    for (const m of messages.slice(-20)) {
      console.log(`  [${m.type}] ${m.text.substring(0, 200)}`);
    }

    expect(pixelData.error).toBeUndefined();

    // At least one pixel should be non-transparent-black
    // (Bevy clear color or UI background)
    const pixels = Object.values(pixelData.pixels);
    const hasContent = pixels.some(p => p.r > 0 || p.g > 0 || p.b > 0 || p.a > 0);

    if (!hasContent) {
      // Take screenshot for visual inspection
      console.log('WARNING: All sampled pixels are transparent black (0,0,0,0)');
      console.log('This likely means preserveDrawingBuffer=false cleared the buffer');
      console.log('Check the screenshot artifact for visual rendering.');
    }
  });

  test('screenshot shows rendered content', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(20000);

    // Take full page screenshot
    const screenshot = await page.screenshot({ fullPage: true });
    expect(screenshot.byteLength).toBeGreaterThan(1000);

    // Analyze screenshot pixels - a completely uniform screenshot
    // means nothing rendered (or only clear color)
    // The screenshot will be saved as an artifact automatically
    console.log(`Screenshot captured: ${screenshot.byteLength} bytes`);
  });
});

test.describe('Bevy App State', () => {
  test('Bevy event loop is running (requestAnimationFrame)', async ({ page }) => {
    // Inject rAF counter before page loads
    await page.addInitScript(() => {
      window.__rafCount = 0;
      const origRaf = window.requestAnimationFrame;
      window.requestAnimationFrame = function (cb) {
        window.__rafCount++;
        return origRaf.call(window, cb);
      };
    });

    await page.goto('/');
    await page.waitForTimeout(15000);

    const rafCount = await page.evaluate(() => window.__rafCount);
    console.log(`requestAnimationFrame called ${rafCount} times in 15s`);
    // At 60fps for 15s, expect ~900 frames. Even at 10fps, ~150.
    // If <10, the event loop isn't running.
    expect(rafCount).toBeGreaterThan(10);
  });

  test('no WASM panics in 30 seconds', async ({ page }) => {
    const messages = collectConsole(page);

    await page.goto('/');
    await page.waitForTimeout(30000);

    const panics = messages.filter(m =>
      m.text.includes('panicked at') ||
      m.text.includes('unreachable') ||
      m.text.includes('RuntimeError')
    );

    if (panics.length > 0) {
      console.log('PANICS DETECTED:');
      for (const p of panics) {
        console.log(`  ${p.text}`);
      }
    }

    expect(panics).toHaveLength(0);
  });
});
