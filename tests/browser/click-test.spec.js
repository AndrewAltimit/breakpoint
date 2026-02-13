import { test, expect } from '@playwright/test';

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

test.describe('Button Click Diagnostics', () => {
  test('canvas receives pointer events', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(10000); // Wait for Bevy to initialize

    // Add a click listener on the canvas to verify events reach it
    const clickReceived = await page.evaluate(() => {
      return new Promise(resolve => {
        const canvas = document.getElementById('game-canvas');
        if (!canvas) { resolve({ error: 'no canvas' }); return; }

        let received = false;
        canvas.addEventListener('pointerdown', (e) => {
          received = true;
          console.log(`Canvas pointerdown: (${e.clientX}, ${e.clientY}) button=${e.button}`);
        }, { once: true });

        // Simulate a click on the canvas center
        canvas.dispatchEvent(new PointerEvent('pointerdown', {
          clientX: canvas.clientWidth / 2,
          clientY: canvas.clientHeight / 2,
          button: 0,
          bubbles: true,
        }));

        setTimeout(() => resolve({ received }), 100);
      });
    });

    console.log('Canvas pointerdown event received:', JSON.stringify(clickReceived));
    expect(clickReceived.received).toBe(true);
  });

  test('click on Create Room button area', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(12000); // Wait for full lobby render

    // Take screenshot before clicking to see button positions
    const beforeScreenshot = await page.screenshot();
    console.log(`Before click screenshot: ${beforeScreenshot.length} bytes`);

    // From the lobby screenshot, "Create Room" button is roughly at center-left
    // Canvas is 1280x720, buttons are centered vertically around y=430-ish
    // Let's click at the approximate location of "Create Room"
    const canvas = page.locator('#game-canvas');
    const box = await canvas.boundingBox();
    console.log(`Canvas bounding box: x=${box.x}, y=${box.y}, w=${box.width}, h=${box.height}`);

    // First click to focus the canvas
    await canvas.click({ position: { x: 10, y: 10 } });
    await page.waitForTimeout(500);

    // Click where "Create Room" button should be (center area, slightly above middle)
    // Based on the screenshot: buttons are at roughly 60% down, centered
    const createRoomX = box.width * 0.38; // Create Room is left of center
    const createRoomY = box.height * 0.59; // Buttons are ~59% down
    console.log(`Clicking Create Room at: (${createRoomX}, ${createRoomY})`);

    await canvas.click({ position: { x: createRoomX, y: createRoomY } });
    await page.waitForTimeout(2000);

    // Take screenshot after clicking
    const afterScreenshot = await page.screenshot();
    console.log(`After click screenshot: ${afterScreenshot.length} bytes`);

    // Check if any new messages appeared (like WebSocket connection attempts)
    const relevantMsgs = messages.filter(m =>
      m.text.includes('WebSocket') ||
      m.text.includes('connect') ||
      m.text.includes('error') ||
      m.text.includes('Room') ||
      m.text.includes('ws://') ||
      m.text.includes('wss://')
    );
    console.log(`Relevant messages after click: ${relevantMsgs.length}`);
    for (const m of relevantMsgs) {
      console.log(`  [${m.type}] ${m.text.substring(0, 300)}`);
    }

    // Print ALL messages for full context
    console.log(`\nAll messages (${messages.length}):`);
    for (const m of messages) {
      console.log(`  [${m.type}] ${m.text.substring(0, 300)}`);
    }
  });

  test('verify Bevy processes mouse input events', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(12000);

    // Check if the canvas has focus
    const focusInfo = await page.evaluate(() => {
      const canvas = document.getElementById('game-canvas');
      return {
        hasFocus: document.activeElement === canvas,
        activeElement: document.activeElement?.tagName,
        tabIndex: canvas?.tabIndex,
        style: {
          pointerEvents: window.getComputedStyle(canvas).pointerEvents,
          display: window.getComputedStyle(canvas).display,
          zIndex: window.getComputedStyle(canvas).zIndex,
        },
      };
    });
    console.log('Focus info:', JSON.stringify(focusInfo, null, 2));

    // Click canvas to give it focus
    const canvas = page.locator('#game-canvas');
    await canvas.click({ position: { x: 640, y: 360 } });
    await page.waitForTimeout(500);

    const focusAfter = await page.evaluate(() => ({
      hasFocus: document.activeElement === document.getElementById('game-canvas'),
      activeElement: document.activeElement?.tagName,
    }));
    console.log('Focus after click:', JSON.stringify(focusAfter));

    // Try multiple clicks on different buttons
    const buttonPositions = [
      { name: 'Mini-Golf', x: 0.39, y: 0.35 },
      { name: 'Platform Racer', x: 0.50, y: 0.35 },
      { name: 'Laser Tag', x: 0.61, y: 0.35 },
      { name: 'Create Room', x: 0.37, y: 0.59 },
      { name: 'Join Room', x: 0.51, y: 0.59 },
      { name: 'Editor', x: 0.64, y: 0.59 },
    ];

    const box = await canvas.boundingBox();
    for (const btn of buttonPositions) {
      const x = Math.round(box.width * btn.x);
      const y = Math.round(box.height * btn.y);
      console.log(`Clicking "${btn.name}" at (${x}, ${y})`);
      await canvas.click({ position: { x, y } });
      await page.waitForTimeout(300);
    }

    await page.waitForTimeout(2000);

    // Screenshot after all clicks
    const screenshot = await page.screenshot();
    console.log(`Screenshot after clicks: ${screenshot.length} bytes`);

    // Check for any interaction-related messages
    const allMsgs = messages.filter(m =>
      m.text.includes('WebSocket') ||
      m.text.includes('connect') ||
      m.text.includes('error') ||
      m.text.includes('Room') ||
      m.text.includes('Editor')
    );
    console.log(`\nInteraction messages: ${allMsgs.length}`);
    for (const m of allMsgs) {
      console.log(`  [${m.type}] ${m.text.substring(0, 300)}`);
    }
  });

  test('check for elements above canvas blocking clicks', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(5000);

    const overlayInfo = await page.evaluate(() => {
      const canvas = document.getElementById('game-canvas');
      const overlay = document.getElementById('overlay');
      const canvasRect = canvas?.getBoundingClientRect();
      const overlayRect = overlay?.getBoundingClientRect();

      // Check what element is at click positions
      const elementsAtCenter = document.elementsFromPoint(
        canvasRect.left + canvasRect.width / 2,
        canvasRect.top + canvasRect.height / 2
      );
      const elementsAtButton = document.elementsFromPoint(
        canvasRect.left + canvasRect.width * 0.38,
        canvasRect.top + canvasRect.height * 0.59
      );

      return {
        canvas: {
          rect: { x: canvasRect.x, y: canvasRect.y, w: canvasRect.width, h: canvasRect.height },
          pointerEvents: window.getComputedStyle(canvas).pointerEvents,
          zIndex: window.getComputedStyle(canvas).zIndex,
        },
        overlay: overlay ? {
          rect: { x: overlayRect.x, y: overlayRect.y, w: overlayRect.width, h: overlayRect.height },
          pointerEvents: window.getComputedStyle(overlay).pointerEvents,
          zIndex: window.getComputedStyle(overlay).zIndex,
          display: window.getComputedStyle(overlay).display,
        } : null,
        elementsAtCenter: elementsAtCenter.map(el => ({
          tag: el.tagName,
          id: el.id,
          className: el.className,
          pointerEvents: window.getComputedStyle(el).pointerEvents,
        })),
        elementsAtButton: elementsAtButton.map(el => ({
          tag: el.tagName,
          id: el.id,
          className: el.className,
          pointerEvents: window.getComputedStyle(el).pointerEvents,
        })),
      };
    });

    console.log('Canvas info:', JSON.stringify(overlayInfo.canvas, null, 2));
    console.log('Overlay info:', JSON.stringify(overlayInfo.overlay, null, 2));
    console.log('Elements at center:', JSON.stringify(overlayInfo.elementsAtCenter, null, 2));
    console.log('Elements at button pos:', JSON.stringify(overlayInfo.elementsAtButton, null, 2));
  });
});
