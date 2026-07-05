# MCP Hosting

`crates/enabot-mcp` exposes ROLA Mini control over Streamable HTTP MCP.

- `list_robots` - list account-bound robots.
- `status` - return the current Mini session.
- `stop` - send an immediate stop.
- `drive`, `forward`, `backward`, `turn_left`, `turn_right`, and `wiggle` -
  bounded movement.
- `snapshot` - write a JPEG on the MCP host.

Run the server:

```sh
cargo run -p enabot-mcp
```

Local endpoint:

```text
http://127.0.0.1:8788/mcp
```

The host auto-selects the first robot bound to the configured Enabot account. If
you replace the robot, pair the new one in the ROLA app and restart
`enabot-mcp`.

## Cloudflare Tunnel

Keep the MCP server bound to localhost and point Cloudflare Tunnel at
`127.0.0.1:8788`.

```sh
cloudflared tunnel login
cloudflared tunnel create rola-mcp
cloudflared tunnel route dns rola-mcp rola-mcp.alex-netsch.com
```

`~/.cloudflared/rola-mcp.yml`:

```yaml
tunnel: rola-mcp
credentials-file: /Users/alexAthome/.cloudflared/f141ef03-6221-4dfa-a19b-00412553fb23.json

ingress:
  - hostname: rola-mcp.alex-netsch.com
    service: http://127.0.0.1:8788
  - service: http_status:404
```

Run the tunnel:

```sh
cloudflared tunnel --config ~/.cloudflared/rola-mcp.yml run rola-mcp
```

Public endpoint:

```text
https://rola-mcp.alex-netsch.com/mcp
```

## Codex Client

Paste this into `~/.codex/config.toml`:

```toml
[mcp_servers.rola-mcp]
url = "https://rola-mcp.alex-netsch.com/mcp"
```

Or run:

```sh
codex mcp add rola-mcp --url https://rola-mcp.alex-netsch.com/mcp
```

Do not share `.env`, Cloudflare credentials, Enabot credentials, app constants,
captures, or generated tokens.

## Notes

For persistence on macOS, install `cloudflared` as a launchd service after the
tunnel works interactively:

```sh
cloudflared service install
```
