import { defineConfig } from '@playwright/test';

export default defineConfig({
  testDir: '.',
  testMatch: '*.spec.js',
  timeout: 120_000,
  expect: { timeout: 30_000 },
  globalSetup: './global-setup.js',
  // Each test gets its own room, so parallel execution is safe.
  // Workers = 2 is conservative; increase once stability is confirmed.
  fullyParallel: false,
  workers: process.env.CI ? 1 : 2,
  retries: process.env.CI ? 1 : 0,
  reporter: [['list'], ['html', { open: 'never', outputFolder: 'report' }]],
  outputDir: 'results',
  use: {
    baseURL: process.env.BASE_URL || 'http://127.0.0.1:8080',
    screenshot: 'on',
    video: 'retain-on-failure',
    trace: 'retain-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        browserName: 'chromium',
        // WebGL needs a real GPU or swiftshader
        launchOptions: {
          args: [
            '--use-gl=angle',
            '--use-angle=swiftshader',
            '--enable-webgl',
            '--enable-webgl2-compute-context',
            '--enable-unsafe-webgpu',
          ],
        },
      },
    },
    {
      name: 'firefox',
      use: {
        browserName: 'firefox',
        launchOptions: {
          firefoxUserPrefs: {
            'webgl.force-enabled': true,
            'webgl.disabled': false,
          },
        },
      },
    },
  ],
});
