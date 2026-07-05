# MCP and Cloudflare exposure

The repo contains:

- `enabot-sdk`, which has the reusable robot control primitives.
- `enabot-cli`, which wraps those primitives as local commands.
- `enabot-mcp`, which exposes robot control over Streamable HTTP MCP.
- native RTM and RTC sidecars used by both control and snapshots.

## Current MCP server

The workspace includes `crates/enabot-mcp`, a Streamable HTTP MCP server that
depends on `enabot-sdk` and exposes this tool surface:

- `list_robots` - list account-bound robots.
- `status` - return the current Mini session summary.
- `stop` - send an immediate stop command.
- `drive`, `forward`, `backward`, `turn_left`, `turn_right`, and `wiggle` -
  bounded movement commands.
- `snapshot` - slower command that writes a JPEG and returns metadata.

Run it locally with:

```sh
cargo run -p enabot-mcp
```

The default MCP endpoint is:

```text
http://127.0.0.1:8788/mcp
```

For remote access through Cloudflare, keep the server bound to `127.0.0.1` and
publish that local port through Cloudflare Tunnel. Require authentication before
any movement tool is callable.

Movement tools should keep the existing CLI safety limits: clamp speeds, reject
long drive durations, and always send `stop` after a timed drive.

## Cloudflare Tunnel

Use a named Cloudflare Tunnel for a machine that stays near the robot:

```sh
cloudflared tunnel login
cloudflared tunnel create rola-mcp
cloudflared tunnel route dns rola-mcp rola-mcp.example.com
```

Example `cloudflared` config:

```yaml
tunnel: rola-mcp
credentials-file: /Users/alexAthome/.cloudflared/<tunnel-id>.json

ingress:
  - hostname: rola-mcp.example.com
    service: http://127.0.0.1:8788
  - service: http_status:404
```

Run it with:

```sh
cloudflared tunnel --config ~/.cloudflared/rola-mcp.yml run rola-mcp
```

For persistence on macOS, install it as a launchd service after the tunnel works
interactively:

```sh
cloudflared service install
```

## Security notes

Do not expose raw unauthenticated robot control on the public Internet. Put the
tunnel hostname behind Cloudflare Access or equivalent OAuth, and add a second
server-side shared token check for non-browser MCP clients if needed.

The local MCP HTTP server should:

- bind to `127.0.0.1`, not `0.0.0.0`;
- validate `Origin` on HTTP requests;
- log movement and snapshot calls;
- keep `.env`, captures, tunnel credentials, and generated tokens uncommitted.
