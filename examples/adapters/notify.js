#!/usr/bin/env node
/**
 * Breakpoint event notification adapter for Node.js.
 *
 * Usage: node notify.js <title> [event_type] [priority]
 *
 * Environment variables:
 *   BREAKPOINT_URL       - Server URL (default: http://localhost:8080)
 *   BREAKPOINT_API_TOKEN - API authentication token
 */

const BREAKPOINT_URL = process.env.BREAKPOINT_URL || "http://localhost:8080";
const BREAKPOINT_TOKEN = process.env.BREAKPOINT_API_TOKEN || "";

async function notify(title, { eventType = "custom", priority = "ambient", ...extra } = {}) {
  const resp = await fetch(`${BREAKPOINT_URL}/api/v1/events`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${BREAKPOINT_TOKEN}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      id: `js-${Date.now()}`,
      event_type: eventType,
      source: "node-adapter",
      priority,
      title,
      timestamp: new Date().toISOString(),
      ...extra,
    }),
  });

  if (!resp.ok) {
    throw new Error(`Breakpoint API error: ${resp.status} ${resp.statusText}`);
  }
  return resp.json();
}

// CLI usage
const [, , title, eventType, priority] = process.argv;
if (!title) {
  console.error("Usage: notify.js <title> [event_type] [priority]");
  process.exit(1);
}

notify(title, { eventType: eventType || "custom", priority: priority || "ambient" })
  .then((result) => console.log(`Accepted ${result.accepted} event(s): ${result.event_ids}`))
  .catch((err) => {
    console.error(err.message);
    process.exit(1);
  });
