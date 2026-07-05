# rola-reverse

Rust tooling for phone-free Enabot ROLA Mini control.

The SDK logs in to Enabot cloud, requests a fresh Mini session, and sends control
commands through a native Agora RTM sidecar. The Android phone is not part of the
normal control path.

## Layout

- `crates/enabot-sdk` - Enabot login, Mini session, command building, and sidecar transport orchestration.
- `crates/enabot-cli` - command-line control tool.
- `sidecars/native-rtm` - JSON-lines wrapper around Agora native RTM.
- `sidecars/rtc-snapshot-native-macos` - default native macOS RTC sidecar that captures a JPEG from Agora RTC.
- `docs/protocol.md` - protocol notes for login, session, and command messages.
- `docs/native-transport.md` - notes on the native Agora sidecar approach.
- `src/control` - older JS fallback harness kept as a known-good comparison path.

## Setup

Pair the ROLA Mini with the official ROLA app first. The app is still the
onboarding tool for Wi-Fi setup and account binding. After the robot is visible
on the account, this SDK can control it without the phone in the normal path.

Copy the example environment file and fill in local values:

```sh
cp .env.example .env
```

Required `.env` values today:

- Enabot account credentials for an account that can access the robot.
- Reusable ROLA app constants used for request signing, body encryption, and
  Agora.

`ENABOT_DEVICE_ID` is optional. If it is left blank, the SDK generates a stable
local client id in `.enabot/device_id`.

The SDK auto-selects the first account-bound robot returned by Enabot. Run
`enabot robots` after filling those values if you want to inspect what is bound
to the account.

Install sidecar dependencies once:

```sh
cd sidecars/native-rtm
npm install
cd ../..
```

Build the Rust CLI:

```sh
cargo build
```

## Usage

```sh
cargo run -p enabot-cli -- robots
cargo run -p enabot-cli -- wiggle
cargo run -p enabot-cli -- forward --speed 55 --ms 500
cargo run -p enabot-cli -- backward --speed 55 --ms 500
cargo run -p enabot-cli -- turn-left --speed 40 --ms 350
cargo run -p enabot-cli -- turn-right --speed 40 --ms 350
cargo run -p enabot-cli -- drive --ly 55 --rx 0 --ms 500
cargo run -p enabot-cli -- stop
```

Snapshot on macOS:

```sh
cargo run -p enabot-cli -- snapshot --out artifacts/snapshots/latest.jpg
cargo run -p enabot-cli -- snapshot --quality hd --out artifacts/snapshots/hd.jpg
```

The snapshot command sends the `102011` RTM trigger, then joins the Mini RTC
channel and writes the current robot video frame as a JPEG. The default sidecar
uses Agora's native macOS RTC SDK via SwiftPM, so it does not require Chrome.
The first run downloads the pinned Agora binary frameworks and builds the native
helper. `--quality` can request `fluent`, `hd`, or `original` before capturing.

## MCP server

Run the local Streamable HTTP MCP server:

```sh
cargo run -p enabot-mcp
```

Available tools mirror the CLI controls: `list_robots`, `status`, `drive`,
`forward`, `backward`, `turn_left`, `turn_right`, `stop`, `wiggle`, and
`snapshot`.

The MCP host auto-selects the first robot bound to the configured Enabot
account. Hosting and client setup notes live in `docs/mcp-cloudflare.md`.

Codex clients can paste this into `~/.codex/config.toml`:

```toml
[mcp_servers.rola-mcp]
url = "https://rola-mcp.alex-netsch.com/mcp"
```

## Secrets

Never commit `.env`, captures, APKs, cookies, tokens, or extracted app secrets. The
repo ignores those by default.
