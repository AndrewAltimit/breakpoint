#!/bin/bash
# Breakpoint event notification adapter for shell.
#
# Usage: ./notify.sh <title> [event_type] [priority]
#
# Environment variables:
#   BREAKPOINT_URL       - Server URL (default: http://localhost:8080)
#   BREAKPOINT_API_TOKEN - API authentication token

set -euo pipefail

TITLE="${1:?Usage: $0 <title> [event_type] [priority]}"
EVENT_TYPE="${2:-custom}"
PRIORITY="${3:-ambient}"

HOST="${BREAKPOINT_URL:-http://localhost:8080}"
TOKEN="${BREAKPOINT_API_TOKEN:-}"

curl -sf -X POST "${HOST}/api/v1/events" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"id\": \"sh-$(date +%s%N)\",
    \"event_type\": \"${EVENT_TYPE}\",
    \"source\": \"shell\",
    \"priority\": \"${PRIORITY}\",
    \"title\": \"${TITLE}\",
    \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"
  }"

echo ""
