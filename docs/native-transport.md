# Native Agora Transports

The native transport proof used Agora RTM 2.2.8 through a native macOS arm64 wrapper.
It did not use the phone, Web SDK, or REST peer messaging.

## Proof Result

The native client:

1. Logged in with `AGORA_APP_ID`, `app_rtm_uid`, and `app_rtm_token`.
2. Sent the same wiggle sequence as the browser harness:
   `enter_live`, `nudge_forward`, `stop`, `nudge_back`, `stop`.
3. Received robot messages from `mini_rtm_uid`, including `101004`, `101006`, and `101026`.

That proves native Agora RTM is the right transport path for the SDK.

## Native RTC Snapshot

Snapshot media now uses a native macOS RTC sidecar:

```text
Rust SDK -> stdin JSON -> rtc-snapshot-native-macos -> Agora RTC -> JPEG
```

The sidecar is Swift because Agora's current macOS SDK exposes the useful raw
remote-frame API as Objective-C/Swift:

- `setVideoFrameDelegate`
- `onRenderVideoFrame(_:uid:channelId:)`
- `getVideoFormatPreference` returning RGBA
- `getObservedFramePosition` returning pre-renderer frames

This avoids a custom C++ bridge and keeps the native media adapter small. Rust
still owns Enabot login, Mini session acquisition, RTM triggers, retries, and CLI
surface. If the sidecar boundary becomes inconvenient later, the likely next step
is Rust calling the same Objective-C API with `objc2`, not a new C++ layer.

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
- RTC snapshot trigger/capture orchestration

Once this is stable, direct Rust FFI can replace the sidecar if packaging one binary
becomes more important than keeping the integration simple.

## Current Commands

```sh
npm install --prefix sidecars/native-rtm
cargo run -p enabot-cli -- session
cargo run -p enabot-cli -- wiggle
cargo run -p enabot-cli -- forward --speed 55 --ms 500
cargo run -p enabot-cli -- turn-left --speed 40 --ms 350
cargo run -p enabot-cli -- snapshot --out artifacts/snapshots/latest.jpg
```

The `wiggle` command performs the full live path:

1. Enabot login.
2. Mini session request.
3. Native RTM login.
4. `enter_live`, forward, stop, back, stop.
5. Collect robot replies.

The directional commands use the same path but send one timed movement followed
by `stop`.
