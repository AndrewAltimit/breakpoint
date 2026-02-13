import { test, expect } from '@playwright/test';

function collectConsole(page) {
  const messages = [];
  page.on('console', msg => {
    messages.push({ type: msg.type(), text: msg.text() });
  });
  return messages;
}

// Test at DPR=2 to verify the winit fix
test.use({ deviceScaleFactor: 2, viewport: { width: 960, height: 540 } });

test('canvas dimensions correct at DPR 2', async ({ page }) => {
  await page.goto('/');
  await page.waitForTimeout(12000);

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

test('Create Room button works at DPR 2', async ({ page }) => {
  const messages = collectConsole(page);
  await page.goto('/');
  await page.waitForTimeout(12000);

  const canvas = page.locator('#game-canvas');
  const box = await canvas.boundingBox();

  // Scan the button area to find "Create Room"
  // At 960x540, buttons are roughly centered. Try a range of positions.
  let hitFound = false;
  for (let yPct = 0.55; yPct <= 0.70; yPct += 0.02) {
    for (let xPct = 0.20; xPct <= 0.45; xPct += 0.02) {
      const beforeCount = messages.filter(m => m.text.includes('WebSocket')).length;

      const x = Math.round(box.width * xPct);
      const y = Math.round(box.height * yPct);
      await canvas.click({ position: { x, y } });
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
    await page.waitForTimeout(12000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // At DPR=1 1280x720, buttons at roughly 38% x, 59% y
    const x = Math.round(box.width * 0.38);
    const y = Math.round(box.height * 0.59);
    console.log(`Clicking at (${x}, ${y})`);
    await canvas.click({ position: { x, y } });
    await page.waitForTimeout(2000);

    const wsConnected = messages.some(m => m.text.includes('WebSocket connected'));
    console.log(`WebSocket connected: ${wsConnected}`);
    expect(wsConnected).toBe(true);
  });
});
