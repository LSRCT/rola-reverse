# Enabot ROLA Mini Protocol

This is the protocol state we have actually proved for the current robot/account.
Do not store raw passwords, cookies, RTM tokens, or extracted app secrets in tracked files.

## Runtime Goal

The phone is not required for the normal control path anymore:

1. Log in to Enabot cloud from local credentials.
2. Request a fresh Mini session for the robot.
3. Log in to Agora RTM with the Enabot-issued RTM uid/token.
4. Send JSON control messages to the robot RTM peer.

A phone may still be useful for future reverse engineering, such as pairing flows,
media/photo retrieval, or newly introduced firmware commands. It is not required for
login, session creation, or basic motion control.

## Durable Configuration

These values belong in local `.env`:

- `ENABOT_ACCOUNT`
- `ENABOT_PASSWORD`
- `ENABOT_APP_TOKEN`
- `ENABOT_ACCESS_KEY_SECRET_S2`
- `ENABOT_BODY_ENCRYPT_KEY_S2`
- `AGORA_APP_ID`

Optional/defaulted values:

- `ENABOT_LOGIN_REGION`
- `ENABOT_PHONE_AREA`
- `ENABOT_LANGUAGE`
- `ENABOT_ACCEPT_LANGUAGE`
- `ENABOT_DEVICE_ID` - generated in `.enabot/device_id` when unset
- `ENABOT_ROBOT_ID` - needed for control commands; discover with `enabot robots`

## Enabot Login

Endpoint:

```text
POST https://ebo.enabotserverintl.com/api/v1/s1/users/login/
```

The request body is a signed/encrypted envelope:

- AES-128-GCM encrypts the login payload.
- The 16-byte IV is prepended to ciphertext and auth tag, then base64 encoded.
- HMAC-SHA1 signs the canonical request envelope.
- Login response sets runtime cookies including `sessionid` and `csrftoken`.

Cookies are runtime state. They are produced by login and should be cached only as
short-lived session material, not treated as durable extracted secrets.

## Mini Session

Endpoint:

```text
POST https://ebo.enabotserverintl.com/api/v1/rola/mini/session
```

Body:

```json
{
  "require_online_status": true,
  "robot_id": 123456
}
```

Requires Enabot cookies from login. The response contains:

- `sid`
- `app_rtm_uid`
- `app_rtm_token`
- `mini_rtm_uid`
- `app_rtc_uid`
- `app_rtc_token`
- `rtc_channel`
- `mini_rtc_uid`
- `is_online`

These session values are ephemeral. Refresh them when expired or when Agora login fails.

## RTM Control Messages

Known robot peer shape:

```text
mini_rtm_uid = us_prod_<robot_id>
```

Known app uid shape:

```text
app_rtm_uid = us_prod_<account/device suffix>
```

Known command ids:

- `101003` - enter live/control session.
- `101005` - heartbeat/state.
- `101007` - movement/joystick.
- `102055` - set live video quality. The app labels values as Fluent (`1`),
  HD (`2`), hidden Super (`3`), and Original (`4`).
- `102011` - snapshot request. The robot accepts the trigger, but the useful
  image path is RTC frame capture, not an RTM image response.

Enter live:

```json
{
  "id": 101003,
  "sid": "<sid>",
  "data": { "userId": 123456 },
  "type": 0,
  "timestamp": 1700000000000
}
```

Forward movement:

```json
{
  "id": 101007,
  "sid": "<sid>",
  "data": { "lx": 0, "ly": -55, "rx": 0, "ry": 0, "buttons": 1 },
  "type": 0,
  "timestamp": 1700000000500
}
```

Stop:

```json
{
  "id": 101007,
  "sid": "<sid>",
  "data": { "lx": 0, "ly": 0, "rx": 0, "ry": 0, "buttons": 1 },
  "type": 0,
  "timestamp": 1700000001000
}
```

Robot replies observed:

- `101004` - device info/status.
- `101006` - state acknowledgement.
- `101026` - battery, storage, Wi-Fi/status heartbeat.
- `102056` - set-video-quality acknowledgement. `data.status` was `0` and
  `data.videoQuality` echoed the requested value in live tests.

Set live video quality:

```json
{
  "id": 102055,
  "sid": "<sid>",
  "data": { "videoQuality": 2 },
  "type": 0,
  "timestamp": 1700000001500
}
```

## Snapshot

The practical phone-free snapshot path is:

1. Log in and request a fresh Mini session.
2. Connect to RTM with `app_rtm_uid` / `app_rtm_token`.
3. Send `101003` enter-live to `mini_rtm_uid`.
4. Optionally send `102055` set-video-quality to `mini_rtm_uid`.
5. Send `102011` snapshot trigger to `mini_rtm_uid`.
6. Join the Agora RTC channel with `app_rtc_uid`, `app_rtc_token`, and `rtc_channel`.
7. Subscribe to the robot video publisher (`mini_rtc_uid`).
8. Capture the current remote video frame and write it as JPEG.

Live proof from July 5, 2026:

- RTC channel: `mini_us_prod_<robot_id>`.
- App RTC uid: numeric `app_rtc_uid`.
- Robot video publisher: numeric `mini_rtc_uid`.
- `102055` with `videoQuality: 1` returned `102056` with `videoQuality: 1`
  and captured `640x360`.
- `102055` with `videoQuality: 2` returned `102056` with `videoQuality: 2`
  and captured `1280x720`.
- `102055` with `videoQuality: 4` returned `102056` with `videoQuality: 4`
  and captured `2304x1296`.
- No image bytes or download URL were observed over RTM after `102011`.

The preferred implementation is the native macOS RTC sidecar. It uses Agora's
current macOS SDK through SwiftPM, registers an `AgoraVideoFrameDelegate`, requests
RGBA frames at `AgoraVideoFramePositionPreRenderer`, and writes the first matching
publisher frame with ImageIO.

The SD/media path may still exist for app-created/stored photos, but it is a
separate reverse-engineering task. The SDK snapshot command currently treats
`102011` as a trigger/marker and gets the file from the live RTC frame.

## Transport Findings

Working:

- Agora Web RTM SDK in a browser harness.
- Native Agora RTM SDK through a macOS arm64 native wrapper.
- Native Agora RTC SDK through a macOS arm64 SwiftPM sidecar for snapshots.

Not working for control:

- Agora REST peer messaging with Enabot RTM token. Auth was accepted with
  `Authorization: agora token=...`, but messages returned `message_offline` and did
  not produce robot replies.

Preferred SDK architecture:

1. Rust owns Enabot login, Mini session, command modeling, retries, and public API.
2. A small native Agora RTM sidecar handles login/publish/subscribe initially.
3. A native macOS Agora RTC sidecar handles snapshot frame capture.
4. Later, replace sidecars with direct Rust FFI if that is worth the packaging cost.
