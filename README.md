# enabot_claude

Phone-free control experiments for an Enabot ROLA Mini.

## Status

The useful path is now proven:

- Enabot cloud login can be replayed from local credentials.
- The Mini session endpoint returns fresh Agora RTM/RTC session material.
- Basic robot control works without the phone through Agora RTM.
- Native Agora RTM also works, so the SDK path is Rust core plus a native RTM sidecar/FFI.

The old LAN/TUTK probing and phone-assisted control paths were removed because they did not
lead to the current SDK architecture.

## Current Layout

- `docs/protocol.md` - known Enabot login/session/control protocol.
- `docs/native-transport.md` - transport decision and native RTM proof.
- `src/control/enabot_login.js` - proven login envelope replay.
- `src/control/rola_rtm_server.js` and `src/control/rola_rtm_harness.html` - browser RTM fallback/test oracle.

## Secrets

Copy `.env.example` to `.env` and fill in real values locally. `.env`, captures, APKs,
tokens, cookies, and generated artifacts are gitignored.
