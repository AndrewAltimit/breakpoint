import { test, expect } from '@playwright/test';

function collectConsole(page) {
  const messages = [];
  page.on('console', msg => {
    messages.push({ type: msg.type(), text: msg.text() });
  });
  return messages;
}

// Test with device scale factor 2 (HiDPI)
test.use({ deviceScaleFactor: 2, viewport: { width: 960, height: 540 } });

test.describe('HiDPI Button Click Test (DPR=2)', () => {
  test('buttons respond to clicks at DPR 2', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(12000);

    // Get canvas info
    const canvasInfo = await page.evaluate(() => {
      const c = document.getElementById('game-canvas');
      return {
        cssWidth: c.clientWidth,
        cssHeight: c.clientHeight,
        attrWidth: c.width,
        attrHeight: c.height,
        dpr: window.devicePixelRatio,
        boundingBox: c.getBoundingClientRect(),
      };
    });
    console.log('Canvas info at DPR 2:', JSON.stringify(canvasInfo, null, 2));

    // Take before screenshot
    const before = await page.screenshot();
    console.log(`Before screenshot: ${before.length} bytes`);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();
    console.log(`Canvas box: ${box.width}x${box.height}`);

    // Click on Create Room (approximate position based on ratio)
    const createX = Math.round(box.width * 0.38);
    const createY = Math.round(box.height * 0.59);
    console.log(`Clicking Create Room at: (${createX}, ${createY})`);
    await canvas.click({ position: { x: createX, y: createY } });
    await page.waitForTimeout(3000);

    const after = await page.screenshot();
    console.log(`After screenshot: ${after.length} bytes`);

    // Check for WebSocket connected message
    const wsMsg = messages.filter(m => m.text.includes('WebSocket'));
    console.log(`WebSocket messages: ${wsMsg.length}`);
    for (const m of wsMsg) {
      console.log(`  [${m.type}] ${m.text}`);
    }

    // Now test Editor button
    const editorX = Math.round(box.width * 0.64);
    const editorY = Math.round(box.height * 0.59);
    console.log(`Clicking Editor at: (${editorX}, ${editorY})`);
    await canvas.click({ position: { x: editorX, y: editorY } });
    await page.waitForTimeout(2000);

    const editorScreenshot = await page.screenshot();
    console.log(`Editor screenshot: ${editorScreenshot.length} bytes`);

    console.log(`\nAll messages (${messages.length}):`);
    for (const m of messages) {
      console.log(`  [${m.type}] ${m.text.substring(0, 200)}`);
    }
  });

  test('check exact button hit areas with grid click', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(12000);

    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();

    // Click a grid of points in the button area to find exact hit positions
    console.log(`Canvas: ${box.width}x${box.height} at DPR ${await page.evaluate(() => window.devicePixelRatio)}`);

    // Scan the area where buttons should be (roughly 35%-65% x, 55%-65% y)
    const results = [];
    for (let yPct = 0.50; yPct <= 0.68; yPct += 0.02) {
      for (let xPct = 0.25; xPct <= 0.75; xPct += 0.05) {
        const x = Math.round(box.width * xPct);
        const y = Math.round(box.height * yPct);

        const beforeCount = messages.length;
        await canvas.click({ position: { x, y } });
        await page.waitForTimeout(200);
        const afterCount = messages.length;

        if (afterCount > beforeCount) {
          const newMsgs = messages.slice(beforeCount).map(m => m.text).join('; ');
          results.push({ x, y, xPct: xPct.toFixed(2), yPct: yPct.toFixed(2), msgs: newMsgs });
          console.log(`HIT at (${x},${y}) [${xPct.toFixed(2)},${yPct.toFixed(2)}]: ${newMsgs.substring(0, 100)}`);
        }
      }
    }

    console.log(`\nTotal hits: ${results.length}`);
    if (results.length === 0) {
      console.log('NO BUTTON HITS DETECTED - buttons are not responding at DPR 2!');
    }
  });
});
