# enabot_claude

Phone-free control experiments for an Enabot ROLA Mini.

## Status

The useful path is now proven:

- Enabot cloud login can be replayed from local credentials.
- The Mini session endpoint returns fresh Agora RTM/RTC session material.
- Basic robot control works without the phone through Agora RTM.
- Native Agora RTM also works, so the SDK path is Rust core plus a native RTM sidecar/FFI.
- The Rust CLI can run a live native-RTM wiggle with `cargo run -p enabot-cli -- wiggle`.

The old LAN/TUTK probing and phone-assisted control paths were removed because they did not
lead to the current SDK architecture.

## Current Layout

- `docs/protocol.md` - known Enabot login/session/control protocol.
- `docs/native-transport.md` - transport decision and native RTM proof.
- `crates/enabot-sdk` - Rust login/session/command/sidecar transport core.
- `crates/enabot-cli` - Rust CLI for live control checks.
- `sidecars/native-rtm` - JSON-lines sidecar around Agora native RTM.
- `src/control/enabot_login.js` - original proven login envelope replay.
- `src/control/rola_rtm_server.js` and `src/control/rola_rtm_harness.html` - browser RTM fallback/test oracle.

## Live Check

Install sidecar dependencies once:

```sh
cd sidecars/native-rtm
npm install
cd ../..
```

Then run:

```sh
cargo run -p enabot-cli -- wiggle
```

Drive commands:

```sh
cargo run -p enabot-cli -- forward --speed 55 --ms 500
cargo run -p enabot-cli -- backward --speed 55 --ms 500
cargo run -p enabot-cli -- turn-left --speed 40 --ms 350
cargo run -p enabot-cli -- turn-right --speed 40 --ms 350
cargo run -p enabot-cli -- drive --ly 55 --rx 0 --ms 500
cargo run -p enabot-cli -- stop
```

## Secrets

Copy `.env.example` to `.env` and fill in real values locally. `.env`, captures, APKs,
tokens, cookies, and generated artifacts are gitignored.
