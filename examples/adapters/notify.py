#!/usr/bin/env python3
"""Breakpoint event notification adapter for Python."""

import datetime
import os
import sys

import requests

BREAKPOINT_URL = os.environ.get("BREAKPOINT_URL", "http://localhost:8080")
BREAKPOINT_TOKEN = os.environ.get("BREAKPOINT_API_TOKEN", "")


def notify(title, event_type="custom", priority="ambient", **kwargs):
    """Send an event to Breakpoint.

    Args:
        title: Short display title for the event.
        event_type: Event type string (e.g., "pipeline.failed").
        priority: Priority tier: ambient, notice, urgent, critical.
        **kwargs: Additional event fields (body, url, actor, tags, etc.)

    Returns:
        Response JSON with accepted count and event IDs.
    """
    resp = requests.post(
        f"{BREAKPOINT_URL}/api/v1/events",
        headers={"Authorization": f"Bearer {BREAKPOINT_TOKEN}"},
        json={
            "id": f"py-{int(datetime.datetime.now().timestamp() * 1000)}",
            "event_type": event_type,
            "source": "python-adapter",
            "priority": priority,
            "title": title,
            "timestamp": datetime.datetime.utcnow().isoformat() + "Z",
            **kwargs,
        },
        timeout=10,
    )
    resp.raise_for_status()
    return resp.json()


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(f"Usage: {sys.argv[0]} <title> [event_type] [priority]")
        sys.exit(1)

    title = sys.argv[1]
    event_type = sys.argv[2] if len(sys.argv) > 2 else "custom"
    priority = sys.argv[3] if len(sys.argv) > 3 else "ambient"

    result = notify(title, event_type=event_type, priority=priority)
    print(f"Accepted {result['accepted']} event(s): {result['event_ids']}")
