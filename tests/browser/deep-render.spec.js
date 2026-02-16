import { test, expect } from '@playwright/test';
import { writeFileSync } from 'fs';
import { join } from 'path';

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

test('save canvas toDataURL image', async ({ page }) => {
  await page.goto('/');
  await page.waitForTimeout(20000);

  const dataUrl = await page.evaluate(() => {
    const c = document.getElementById('game-canvas');
    return c ? c.toDataURL('image/png') : null;
  });

  if (dataUrl) {
    const base64 = dataUrl.split(',')[1];
    const buf = Buffer.from(base64, 'base64');
    const outPath = join(import.meta.dirname, 'results', 'canvas-capture.png');
    writeFileSync(outPath, buf);
    console.log(`Saved canvas capture: ${outPath} (${buf.length} bytes)`);
  }
});

test('dense pixel scan - find non-background pixels', async ({ page }) => {
  await page.goto('/');
  await page.waitForTimeout(20000);

  const result = await page.evaluate(() => {
    return new Promise(resolve => {
      // In Firefox, rAF may never fire if WASM uses an internal event loop path.
      // Fall back to scanning immediately after a timeout.
      const doScan = () => {
        const c = document.getElementById('game-canvas');
        const gl = c?.getContext('webgl2');
        if (!gl) { resolve({ error: 'no gl' }); return; }

        const w = gl.drawingBufferWidth;
        const h = gl.drawingBufferHeight;
        const px = new Uint8Array(4);

        // Get the background color first (top-left corner)
        gl.readPixels(5, 5, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
        const bgR = px[0], bgG = px[1], bgB = px[2];

        // Scan every 4th pixel for anything different from bg
        const nonBgPixels = [];
        const step = 4;
        for (let y = 0; y < h && nonBgPixels.length < 100; y += step) {
          for (let x = 0; x < w && nonBgPixels.length < 100; x += step) {
            gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
            if (Math.abs(px[0] - bgR) > 5 || Math.abs(px[1] - bgG) > 5 || Math.abs(px[2] - bgB) > 5) {
              nonBgPixels.push({ x, y, r: px[0], g: px[1], b: px[2], a: px[3] });
            }
          }
        }

        resolve({
          width: w,
          height: h,
          bgColor: [bgR, bgG, bgB],
          nonBgCount: nonBgPixels.length,
          nonBgSamples: nonBgPixels.slice(0, 20),
        });
      };

      // Try rAF first, fall back to setTimeout if rAF doesn't fire
      let fired = false;
      requestAnimationFrame(() => { fired = true; doScan(); });
      setTimeout(() => { if (!fired) doScan(); }, 3000);
    });
  });

  console.log(`Canvas: ${result.width}x${result.height}`);
  console.log(`Background color: rgb(${result.bgColor?.join(',')})`);
  console.log(`Non-background pixels found: ${result.nonBgCount}`);
  if (result.nonBgSamples?.length) {
    for (const p of result.nonBgSamples) {
      console.log(`  (${p.x}, ${p.y}): rgba(${p.r},${p.g},${p.b},${p.a})`);
    }
  } else {
    console.log('ENTIRE CANVAS IS UNIFORM - no UI elements visible');
  }
});

test('test with explicit clear color and no UI', async ({ page }) => {
  // Load the page and check if any Bevy warnings appear
  const messages = collectConsole(page);
  await page.goto('/');
  await page.waitForTimeout(25000);

  // Log all Bevy/wgpu messages
  const bevyMsgs = messages.filter(m =>
    m.text.includes('bevy') || m.text.includes('wgpu') ||
    m.text.includes('Breakpoint') || m.text.includes('warn') ||
    m.text.includes('error') || m.text.includes('ERROR') ||
    m.text.includes('WARN')
  );

  console.log(`=== ${bevyMsgs.length} Bevy/wgpu messages ===`);
  for (const m of bevyMsgs) {
    console.log(`[${m.type}] ${m.text.substring(0, 500)}`);
  }

  // Also check: did tracing get set up?
  console.log(`\nTotal console messages: ${messages.length}`);
  if (messages.length === 0) {
    console.log('WARNING: Zero console messages - tracing-wasm may not be initializing');
    console.log('This could indicate Bevy LogPlugin failed to set up, or tracing subscriber already set');
  }

  // Dump ALL messages
  console.log('\n=== ALL messages ===');
  for (const m of messages) {
    console.log(`[${m.type}] ${m.text.substring(0, 300)}`);
  }
});
