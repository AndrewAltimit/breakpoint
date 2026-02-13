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

test.describe('Rendering Diagnostics', () => {
  test('full console output after 20s', async ({ page }) => {
    const messages = collectConsole(page);
    await page.goto('/');
    await page.waitForTimeout(20000);

    console.log(`=== ${messages.length} console messages ===`);
    for (const m of messages) {
      console.log(`[${m.type}] ${m.text.substring(0, 500)}`);
    }

    // Check for shader compilation issues
    const shaderIssues = messages.filter(m =>
      m.text.toLowerCase().includes('shader') ||
      m.text.toLowerCase().includes('compile') ||
      m.text.toLowerCase().includes('link') ||
      m.text.toLowerCase().includes('glsl') ||
      m.text.toLowerCase().includes('program')
    );
    if (shaderIssues.length > 0) {
      console.log('\n=== SHADER-RELATED MESSAGES ===');
      for (const m of shaderIssues) {
        console.log(`[${m.type}] ${m.text}`);
      }
    }

    // Check for text/font issues
    const fontIssues = messages.filter(m =>
      m.text.toLowerCase().includes('font') ||
      m.text.toLowerCase().includes('text') ||
      m.text.toLowerCase().includes('glyph')
    );
    if (fontIssues.length > 0) {
      console.log('\n=== FONT/TEXT-RELATED MESSAGES ===');
      for (const m of fontIssues) {
        console.log(`[${m.type}] ${m.text}`);
      }
    }
  });

  test('pixel analysis with rAF timing', async ({ page }) => {
    // Read pixels during Bevy's render by using rAF callback
    await page.goto('/');
    await page.waitForTimeout(15000);

    // Sample pixels using requestAnimationFrame to catch them before buffer clear
    const pixelData = await page.evaluate(() => {
      return new Promise((resolve) => {
        requestAnimationFrame(() => {
          const c = document.getElementById('game-canvas');
          if (!c) { resolve({ error: 'no canvas' }); return; }
          const gl = c.getContext('webgl2');
          if (!gl) { resolve({ error: 'no webgl2' }); return; }

          const w = gl.drawingBufferWidth;
          const h = gl.drawingBufferHeight;

          // Sample a grid of pixels
          const samples = {};
          for (let row = 0; row < 5; row++) {
            for (let col = 0; col < 5; col++) {
              const x = Math.floor((col + 0.5) * w / 5);
              const y = Math.floor((row + 0.5) * h / 5);
              const px = new Uint8Array(4);
              gl.readPixels(x, y, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, px);
              samples[`r${row}c${col}`] = [px[0], px[1], px[2], px[3]];
            }
          }

          resolve({ width: w, height: h, samples });
        });
      });
    });

    console.log('Pixel grid (5x5 sample):');
    if (pixelData.samples) {
      const uniqueColors = new Set();
      for (let row = 0; row < 5; row++) {
        const rowStr = [];
        for (let col = 0; col < 5; col++) {
          const px = pixelData.samples[`r${row}c${col}`];
          const hex = `#${px[0].toString(16).padStart(2,'0')}${px[1].toString(16).padStart(2,'0')}${px[2].toString(16).padStart(2,'0')}`;
          rowStr.push(`${hex}(a${px[3]})`);
          uniqueColors.add(hex);
        }
        console.log(`  Row ${row}: ${rowStr.join(' ')}`);
      }
      console.log(`Unique colors: ${uniqueColors.size} - ${[...uniqueColors].join(', ')}`);
      expect(uniqueColors.size).toBeGreaterThanOrEqual(1);
    }
  });

  test('canvas compositing screenshot analysis', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(20000);

    // Read the canvas as a data URL via toDataURL (uses composited image, not WebGL buffer)
    const dataUrl = await page.evaluate(() => {
      const c = document.getElementById('game-canvas');
      if (!c) return null;
      try {
        return c.toDataURL('image/png');
      } catch (e) {
        return 'error: ' + e.message;
      }
    });

    if (dataUrl && dataUrl.startsWith('data:')) {
      // Decode and check if it's a blank image
      const base64 = dataUrl.split(',')[1];
      const bytes = Buffer.from(base64, 'base64');
      console.log(`Canvas toDataURL: ${bytes.length} bytes`);

      // If very small, it's likely a solid color (blank)
      if (bytes.length < 5000) {
        console.log('WARNING: Canvas image is very small - likely solid color / no content');
      } else {
        console.log('Canvas has varied content (likely rendering something)');
      }
    } else {
      console.log('toDataURL result:', dataUrl?.substring(0, 200));
    }

    // Also check page screenshot pixel variance
    const screenshot = await page.screenshot();
    console.log(`Page screenshot: ${screenshot.length} bytes`);
  });

  test('WebGL capabilities and limits', async ({ page }) => {
    await page.goto('/');
    await page.waitForTimeout(5000);

    const caps = await page.evaluate(() => {
      const c = document.getElementById('game-canvas');
      const gl = c?.getContext('webgl2');
      if (!gl) return { error: 'no webgl2' };

      const debugInfo = gl.getExtension('WEBGL_debug_renderer_info');
      return {
        renderer: debugInfo ? gl.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER),
        vendor: debugInfo ? gl.getParameter(debugInfo.UNMASKED_VENDOR_WEBGL) : gl.getParameter(gl.VENDOR),
        version: gl.getParameter(gl.VERSION),
        glslVersion: gl.getParameter(gl.SHADING_LANGUAGE_VERSION),
        maxTextureSize: gl.getParameter(gl.MAX_TEXTURE_SIZE),
        maxCubeMapTextureSize: gl.getParameter(gl.MAX_CUBE_MAP_TEXTURE_SIZE),
        maxRenderbufferSize: gl.getParameter(gl.MAX_RENDERBUFFER_SIZE),
        maxViewportDims: Array.from(gl.getParameter(gl.MAX_VIEWPORT_DIMS)),
        maxTextureImageUnits: gl.getParameter(gl.MAX_TEXTURE_IMAGE_UNITS),
        maxVertexTextureImageUnits: gl.getParameter(gl.MAX_VERTEX_TEXTURE_IMAGE_UNITS),
        maxCombinedTextureImageUnits: gl.getParameter(gl.MAX_COMBINED_TEXTURE_IMAGE_UNITS),
        maxVertexUniformVectors: gl.getParameter(gl.MAX_VERTEX_UNIFORM_VECTORS),
        maxFragmentUniformVectors: gl.getParameter(gl.MAX_FRAGMENT_UNIFORM_VECTORS),
        maxSamples: gl.getParameter(gl.MAX_SAMPLES),
        maxColorAttachments: gl.getParameter(gl.MAX_COLOR_ATTACHMENTS),
        maxDrawBuffers: gl.getParameter(gl.MAX_DRAW_BUFFERS),
        extensions: gl.getSupportedExtensions()?.length || 0,
      };
    });

    console.log('WebGL2 capabilities:', JSON.stringify(caps, null, 2));
  });
});
