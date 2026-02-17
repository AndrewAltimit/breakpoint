/**
 * Playwright global setup.
 *
 * Waits for the breakpoint server to be healthy before running tests.
 * Replaces fixed `waitForTimeout` delays with active health check polling.
 */

const BASE_URL = process.env.BASE_URL || 'http://127.0.0.1:8080';
const TIMEOUT_MS = 15000;
const POLL_INTERVAL_MS = 250;

export default async function globalSetup() {
  const start = Date.now();
  let healthy = false;

  while (Date.now() - start < TIMEOUT_MS) {
    try {
      const resp = await fetch(`${BASE_URL}/health`);
      if (resp.ok) {
        healthy = true;
        break;
      }
    } catch {
      // Server not ready yet
    }
    await new Promise(r => setTimeout(r, POLL_INTERVAL_MS));
  }

  if (!healthy) {
    console.error(
      `Server at ${BASE_URL} did not become healthy within ${TIMEOUT_MS}ms. ` +
      'Make sure the breakpoint server is running.'
    );
    // Don't throw — let tests skip gracefully if server isn't available
  } else {
    const elapsed = Date.now() - start;
    console.log(`Server healthy (${elapsed}ms)`);
  }
}
