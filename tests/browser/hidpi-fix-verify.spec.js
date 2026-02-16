import { test, expect } from '@playwright/test';

function collectConsole(page) {
  const messages = [];
  page.on('console', msg => {
    messages.push({ type: msg.type(), text: msg.text() });
  });
  return messages;
}

// Test at DPR=2 to verify the winit fix.
// Firefox headless + swiftshader at DPR=2 often fails to resize the canvas
// from the default 300x150, so these DPR=2 tests are Chromium-only.
test.use({ deviceScaleFactor: 2, viewport: { width: 960, height: 540 } });
test.describe.configure({ timeout: 120_000 });

test('canvas dimensions correct at DPR 2', async ({ page, browserName }) => {
  test.skip(browserName === 'firefox', 'Firefox headless + swiftshader fails canvas resize at DPR=2');
  await page.goto('/');
  await page.waitForTimeout(15000);

  const info = await page.evaluate(() => {
    const c = document.getElementById('game-canvas');
    return {
      cssWidth: c.clientWidth,
      cssHeight: c.clientHeight,
      attrWidth: c.width,
      attrHeight: c.height,
      dpr: window.devicePixelRatio,
    };
  });

  console.log(`Canvas: CSS ${info.cssWidth}x${info.cssHeight}, attr ${info.attrWidth}x${info.attrHeight}, DPR ${info.dpr}`);

  // Canvas attributes should be CSS * DPR (physical pixels)
  expect(info.attrWidth).toBe(info.cssWidth * info.dpr);
  expect(info.attrHeight).toBe(info.cssHeight * info.dpr);
});

test('Create Room button works at DPR 2', async ({ page, browserName }) => {
  test.skip(browserName === 'firefox', 'Firefox headless + swiftshader fails canvas resize at DPR=2');
  const messages = collectConsole(page);
  await page.goto('/');
  await page.waitForTimeout(15000);

  const canvas = page.locator('#game-canvas');
  const box = await canvas.boundingBox();

  // Scan the button area to find "Create Room". Use force:true to skip
  // Playwright stability waits which take seconds per click under swiftshader.
  let hitFound = false;
  for (let yPct = 0.50; yPct <= 0.70; yPct += 0.02) {
    for (let xPct = 0.40; xPct <= 0.60; xPct += 0.02) {
      const beforeCount = messages.filter(m => m.text.includes('WebSocket')).length;

      const x = Math.round(box.width * xPct);
      const y = Math.round(box.height * yPct);
      await canvas.click({ position: { x, y }, force: true });
      await page.waitForTimeout(300);

      const afterCount = messages.filter(m => m.text.includes('WebSocket')).length;
      if (afterCount > beforeCount) {
        console.log(`Create Room hit at (${x}, ${y}) [${xPct.toFixed(2)}, ${yPct.toFixed(2)}]`);
        hitFound = true;
        break;
      }
    }
    if (hitFound) break;
  }

  expect(hitFound).toBe(true);
});

// Test at DPR=1 (baseline, should still work)
test.describe('DPR 1 baseline', () => {
  test.use({ deviceScaleFactor: 1, viewport: { width: 1280, height: 720 } });

  test('Create Room button works at DPR 1', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(15000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Scan button area to find Create Room. Use force:true to skip
    // stability waits that cause timeouts under swiftshader.
    // Wide scan range to handle Firefox canvas sizing issues.
    let hitFound = false;
    for (let yPct = 0.48; yPct <= 0.68; yPct += 0.02) {
      for (const xPct of [0.48, 0.50, 0.52]) {
        const x = Math.round(box.width * xPct);
        const y = Math.round(box.height * yPct);
        await canvas.click({ position: { x, y }, force: true });
        await page.waitForTimeout(500);

        if (messages.some(m => m.text.includes('WebSocket connected'))) {
          console.log(`Create Room hit at (${x}, ${y})`);
          hitFound = true;
          break;
        }
      }
      if (hitFound) break;
    }

    console.log(`WebSocket connected: ${hitFound}`);
    expect(hitFound).toBe(true);
  });
});
