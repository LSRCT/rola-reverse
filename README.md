# rola-reverse

Rust tooling for phone-free Enabot ROLA Mini control.

The SDK logs in to Enabot cloud, requests a fresh Mini session, and sends control
commands through a native Agora RTM sidecar. The Android phone is not part of the
normal control path.

## Layout

- `crates/enabot-sdk` - Enabot login, Mini session, command building, and sidecar transport orchestration.
- `crates/enabot-cli` - command-line control tool.
- `sidecars/native-rtm` - JSON-lines wrapper around Agora native RTM.
- `docs/protocol.md` - protocol notes for login, session, and command messages.
- `docs/native-transport.md` - notes on the native RTM sidecar approach.
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
- A stable `ENABOT_DEVICE_ID` for the login payload.
- `ENABOT_ROBOT_ID` for the already-paired ROLA Mini.
- Reusable ROLA app constants used for request signing, body encryption, and
  Agora.

Robot discovery is not wired into the CLI yet, so the robot id is currently a
manual setup value. The next ergonomic step is an `enabot robots` command that
lists account-bound robots and allows `ENABOT_ROBOT_ID=auto`.

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
cargo run -p enabot-cli -- wiggle
cargo run -p enabot-cli -- forward --speed 55 --ms 500
cargo run -p enabot-cli -- backward --speed 55 --ms 500
cargo run -p enabot-cli -- turn-left --speed 40 --ms 350
cargo run -p enabot-cli -- turn-right --speed 40 --ms 350
cargo run -p enabot-cli -- drive --ly 55 --rx 0 --ms 500
cargo run -p enabot-cli -- stop
```

## Secrets

Never commit `.env`, captures, APKs, cookies, tokens, or extracted app secrets. The
repo ignores those by default.
