# Breakpoint Integration Guide

This guide covers how to send events to Breakpoint from external systems.

## Authentication

All API requests require a Bearer token:

```
Authorization: Bearer <your-token>
```

Set the token via environment variable `BREAKPOINT_API_TOKEN` or in `breakpoint.toml`:

```toml
[auth]
api_token = "your-secret-token"
```

## Event Schema

```json
{
  "id": "evt-unique-id",
  "event_type": "pipeline.failed",
  "source": "github-actions",
  "priority": "notice",
  "title": "CI failed on main",
  "body": "Test suite failed: 3 failures in auth module",
  "timestamp": "2026-02-12T14:32:00Z",
  "url": "https://github.com/org/repo/actions/runs/12345",
  "actor": "dependabot[bot]",
  "tags": ["ci", "main"],
  "action_required": true,
  "group_key": "github:org/repo:ci",
  "expires_at": "2026-02-12T15:32:00Z",
  "metadata": {
    "is_agent": true,
    "repo": "org/repo"
  }
}
```

### Required Fields

| Field | Type | Description |
|-------|------|-------------|
| `id` | string | Unique event identifier |
| `event_type` | string | Event type (see table below) |
| `source` | string | Source system identifier |
| `title` | string | Short display title |
| `timestamp` | string | ISO 8601 timestamp |

### Optional Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `priority` | string | `"ambient"` | Priority tier: `ambient`, `notice`, `urgent`, `critical` |
| `body` | string | null | Extended description |
| `url` | string | null | Link to source |
| `actor` | string | null | Who/what triggered the event |
| `tags` | string[] | `[]` | Categorization tags |
| `action_required` | boolean | `false` | Whether event needs human action |
| `group_key` | string | null | Group related events (replaces previous in group) |
| `expires_at` | string | null | ISO 8601 expiry time |
| `metadata` | object | `{}` | Arbitrary key-value metadata |

### Event Types

| Type | Category | Description |
|------|----------|-------------|
| `pipeline.started` | CI/CD | Pipeline run started |
| `pipeline.succeeded` | CI/CD | Pipeline completed successfully |
| `pipeline.failed` | CI/CD | Pipeline failed |
| `pr.opened` | GitHub | Pull request opened |
| `pr.reviewed` | GitHub | Pull request reviewed |
| `pr.merged` | GitHub | Pull request merged |
| `pr.conflict` | GitHub | Merge conflict detected |
| `issue.opened` | GitHub | Issue created |
| `issue.assigned` | GitHub | Issue assigned |
| `issue.closed` | GitHub | Issue closed |
| `review.requested` | GitHub | Review requested |
| `deploy.pending` | Deploy | Deployment awaiting approval |
| `deploy.completed` | Deploy | Deployment completed |
| `deploy.failed` | Deploy | Deployment failed |
| `agent.started` | Agent | AI agent started a task |
| `agent.completed` | Agent | AI agent completed a task |
| `agent.blocked` | Agent | AI agent needs human input |
| `agent.error` | Agent | AI agent encountered an error |
| `security.alert` | Security | Security vulnerability detected |
| `test.passed` | Testing | Test suite passed |
| `test.failed` | Testing | Test suite failed |
| `custom` | Custom | Any custom event type |

### Priority Tiers

| Priority | Overlay Behavior | Use Case |
|----------|-----------------|----------|
| `ambient` | Scrolling ticker, no interruption | Routine updates, task completions |
| `notice` | Toast notification (auto-dismiss 8s) | Build failures, review requests |
| `urgent` | Persistent banner until acknowledged | Deploy approvals, agent blocked |
| `critical` | Game pause + modal overlay | Production incidents, security alerts |

## API Endpoints

### POST /api/v1/events

Submit one or more events.

**Single event:**
```bash
curl -X POST https://breakpoint.internal:8080/api/v1/events \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "evt-001",
    "event_type": "pipeline.failed",
    "source": "github-actions",
    "priority": "notice",
    "title": "CI failed on main",
    "timestamp": "2026-02-12T14:32:00Z",
    "action_required": true
  }'
```

**Batch events:**
```bash
curl -X POST https://breakpoint.internal:8080/api/v1/events \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '[
    {"id": "evt-001", "event_type": "test.passed", "source": "ci", "title": "Tests passed", "timestamp": "2026-02-12T14:32:00Z"},
    {"id": "evt-002", "event_type": "deploy.completed", "source": "ci", "title": "Deployed to staging", "timestamp": "2026-02-12T14:33:00Z"}
  ]'
```

**Response (201 Created):**
```json
{
  "accepted": 1,
  "event_ids": ["evt-001"]
}
```

### POST /api/v1/events/:event_id/claim

Claim an event (mark as handled).

```bash
curl -X POST https://breakpoint.internal:8080/api/v1/events/evt-001/claim \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"claimed_by": "alice"}'
```

**Response:**
```json
{
  "claimed": true,
  "event_id": "evt-001"
}
```

### GET /api/v1/events/stream

Server-Sent Events stream for real-time event delivery.

```bash
curl -N https://breakpoint.internal:8080/api/v1/events/stream \
  -H "Authorization: Bearer $TOKEN"
```

Events arrive as SSE with type `alert`:
```
event: alert
id: evt-001
data: {"id":"evt-001","event_type":"pipeline.failed",...}
```

### GET /api/v1/status

Server health check.

```bash
curl https://breakpoint.internal:8080/api/v1/status \
  -H "Authorization: Bearer $TOKEN"
```

### POST /api/v1/webhooks/github

GitHub webhook endpoint. Authenticates via `X-Hub-Signature-256` HMAC. No Bearer token needed.

Configure in GitHub repository Settings > Webhooks:
- **Payload URL:** `https://breakpoint.internal:8080/api/v1/webhooks/github`
- **Content type:** `application/json`
- **Secret:** Your `BREAKPOINT_GITHUB_SECRET` value

Supported GitHub events: `push`, `pull_request`, `workflow_run`, `issues`, `issue_comment`, `check_run`.

## Example Adapters

### Python

```python
import requests
import datetime

def notify_breakpoint(title, event_type="custom", priority="ambient", **kwargs):
    resp = requests.post(
        "https://breakpoint.internal:8080/api/v1/events",
        headers={"Authorization": f"Bearer {TOKEN}"},
        json={
            "id": f"py-{datetime.datetime.now().timestamp()}",
            "event_type": event_type,
            "source": "python-adapter",
            "priority": priority,
            "title": title,
            "timestamp": datetime.datetime.utcnow().isoformat() + "Z",
            **kwargs,
        },
    )
    return resp.json()

# Usage
notify_breakpoint("Tests passed", event_type="test.passed")
notify_breakpoint("Deploy blocked", event_type="deploy.pending", priority="urgent", action_required=True)
```

### Node.js

```javascript
async function notifyBreakpoint(title, { eventType = "custom", priority = "ambient", ...extra } = {}) {
  const resp = await fetch("https://breakpoint.internal:8080/api/v1/events", {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${TOKEN}`,
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
  return resp.json();
}

// Usage
await notifyBreakpoint("PR #42 merged", { eventType: "pr.merged" });
await notifyBreakpoint("Security alert", { eventType: "security.alert", priority: "critical", action_required: true });
```

### Shell (curl)

```bash
#!/bin/bash
# notify.sh - Send a Breakpoint event
TOKEN="${BREAKPOINT_API_TOKEN}"
HOST="${BREAKPOINT_HOST:-localhost:8080}"

curl -s -X POST "http://${HOST}/api/v1/events" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/json" \
  -d "{
    \"id\": \"sh-$(date +%s)\",
    \"event_type\": \"${2:-custom}\",
    \"source\": \"shell\",
    \"priority\": \"${3:-ambient}\",
    \"title\": \"$1\",
    \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"
  }"
```

Usage: `./notify.sh "Build complete" pipeline.succeeded ambient`

## GitHub Actions Integration

Add this step to any workflow to notify Breakpoint:

```yaml
- name: Notify Breakpoint
  if: always()
  run: |
    STATUS=${{ job.status }}
    if [ "$STATUS" = "success" ]; then
      EVENT_TYPE="pipeline.succeeded"
      PRIORITY="ambient"
    else
      EVENT_TYPE="pipeline.failed"
      PRIORITY="notice"
    fi
    curl -s -X POST "${{ secrets.BREAKPOINT_URL }}/api/v1/events" \
      -H "Authorization: Bearer ${{ secrets.BREAKPOINT_API_TOKEN }}" \
      -H "Content-Type: application/json" \
      -d "{
        \"id\": \"gh-${{ github.run_id }}-${{ github.run_attempt }}\",
        \"event_type\": \"${EVENT_TYPE}\",
        \"source\": \"github-actions\",
        \"priority\": \"${PRIORITY}\",
        \"title\": \"${{ github.workflow }} ${STATUS} on ${{ github.repository }}\",
        \"timestamp\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",
        \"url\": \"${{ github.server_url }}/${{ github.repository }}/actions/runs/${{ github.run_id }}\",
        \"actor\": \"${{ github.actor }}\",
        \"tags\": [\"ci\", \"${{ github.repository }}\"],
        \"action_required\": $([ \"$STATUS\" = \"failure\" ] && echo true || echo false)
      }"
```
