# Breakpoint Deployment Guide

## Deployment Modes

### 1. Direct Host (Simplest)

One team member runs the server on their machine. Others connect via LAN IP or VPN.

```bash
# Build and run
cargo build --release -p breakpoint-server
./target/release/breakpoint-server
```

The server listens on `0.0.0.0:8080` by default. Players open `http://<host-ip>:8080` in their browser.

### 2. Docker (Recommended for Teams)

```bash
cd examples/
cp .env.example .env
# Edit .env with your API token

docker compose up -d
```

Or build from source:

```bash
docker build -f docker/server.Dockerfile -t breakpoint .
docker run -p 8080:8080 -e BREAKPOINT_API_TOKEN=your-token breakpoint
```

### 3. Relay Mode (NAT Traversal)

Deploy the relay server on a public host. Players connect through the relay without exposing their machines.

```bash
cargo build --release -p breakpoint-relay
./target/release/breakpoint-relay --port 9090 --max-rooms 50
```

In the game lobby, enter the relay URL (e.g., `wss://relay.example.com:9090/relay`) to create or join a room through the relay.

### 4. Hybrid (Full Infrastructure)

Run the Axum server persistently for event ingestion, webhooks, and SSE streaming. Use it as both the game relay and the alert hub.

```bash
# With GitHub polling enabled
cargo build --release -p breakpoint-server --features github-poller
BREAKPOINT_API_TOKEN=secret ./target/release/breakpoint-server
```

## Configuration

### Config File (breakpoint.toml)

```toml
listen_addr = "0.0.0.0:8080"
web_root = "web"

[auth]
api_token = "your-secret-token"
github_webhook_secret = "your-webhook-secret"

[overlay]
ticker_position = "Top"
dashboard_auto_expand = true

[github]
enabled = true
token = "ghp_your_github_token"
repos = ["org/repo1", "org/repo2"]
poll_interval_secs = 30
agent_patterns = ["*[bot]", "*-agent", "dependabot[bot]"]
```

### Environment Variable Overrides

Environment variables override config file values:

| Variable | Config Key | Default |
|----------|-----------|---------|
| `BREAKPOINT_LISTEN_ADDR` | `listen_addr` | `0.0.0.0:8080` |
| `BREAKPOINT_WEB_ROOT` | `web_root` | `web` |
| `BREAKPOINT_API_TOKEN` | `auth.api_token` | (none) |
| `BREAKPOINT_GITHUB_SECRET` | `auth.github_webhook_secret` | (none) |
| `RUST_LOG` | — | `info` |

## TLS / HTTPS

Breakpoint does not terminate TLS itself. Use a reverse proxy:

### Caddy (automatic HTTPS)

```
breakpoint.example.com {
    reverse_proxy localhost:8080
}
```

### nginx

```nginx
server {
    listen 443 ssl;
    server_name breakpoint.example.com;

    ssl_certificate /etc/ssl/certs/breakpoint.pem;
    ssl_certificate_key /etc/ssl/private/breakpoint.key;

    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

The WebSocket upgrade headers are required for game connectivity.

## Corporate Network Considerations

- **Port 443**: Use a reverse proxy on port 443 for corporate firewall compatibility
- **WSS**: WebSocket Secure (WSS) through the TLS proxy works through most corporate proxies
- **No installs**: Players only need a modern browser — no plugins, extensions, or desktop apps
- **Authentication**: The Bearer token prevents unauthorized event injection

## Docker Production

The production Docker image (`docker/server.Dockerfile`) uses a multi-stage build:

1. **Builder stage**: Compiles the server binary and WASM client
2. **Runtime stage**: Minimal `debian:bookworm-slim` with just the binary, web assets, and WASM bundle

```bash
# Build
docker build -f docker/server.Dockerfile -t breakpoint .

# Run with config file
docker run -p 8080:8080 \
  -v ./breakpoint.toml:/app/breakpoint.toml:ro \
  -e BREAKPOINT_API_TOKEN=secret \
  breakpoint
```

### Docker Compose

See `examples/docker-compose.yml` for a ready-to-use compose file.

## Health Monitoring

The server exposes a status endpoint:

```bash
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/api/v1/status
```

For Docker health checks:

```yaml
healthcheck:
  test: ["CMD", "curl", "-sf", "http://localhost:8080/api/v1/status", "-H", "Authorization: Bearer $TOKEN"]
  interval: 30s
  timeout: 5s
  retries: 3
```
