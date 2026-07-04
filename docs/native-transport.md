# Native RTM Transport

The native transport proof used Agora RTM 2.2.8 through a native macOS arm64 wrapper.
It did not use the phone, Web SDK, or REST peer messaging.

## Proof Result

The native client:

1. Logged in with `AGORA_APP_ID`, `app_rtm_uid`, and `app_rtm_token`.
2. Sent the same wiggle sequence as the browser harness:
   `enter_live`, `nudge_forward`, `stop`, `nudge_back`, `stop`.
3. Received robot messages from `mini_rtm_uid`, including `101004`, `101006`, and `101026`.

That proves native Agora RTM is the right transport path for the SDK.

## Implementation Plan

The repo now starts with a sidecar instead of direct Rust FFI:

```text
Rust SDK -> JSON lines/stdin -> native-rtm-sidecar -> Agora RTM -> robot
```

The sidecar should expose only the transport surface:

- `login`
- `logout`
- `send_user_message`
- incoming robot message stream
- link state events

Rust should own the higher-level Enabot behavior:

- config loading
- Enabot login/session refresh
- typed commands
- command timing
- retries and reconnects
- public SDK API

Once this is stable, direct Rust FFI can replace the sidecar if packaging one binary
becomes more important than keeping the integration simple.

## Current Commands

```sh
npm install --prefix sidecars/native-rtm
cargo run -p enabot-cli -- session
cargo run -p enabot-cli -- wiggle
```

The `wiggle` command performs the full live path:

1. Enabot login.
2. Mini session request.
3. Native RTM login.
4. `enter_live`, forward, stop, back, stop.
5. Collect robot replies.
