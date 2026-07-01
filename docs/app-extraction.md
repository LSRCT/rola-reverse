# The app step — what we need from the ROLA app, and how

**Assumption:** our Mini behaves like the community's EBO SE (TUTK SDK 4.3.6.x).
Under that assumption, everything app/SDK-level is **reused from the community**:

- the 4 TUTK `.so` libs (`libIOTCAPIs/libAVAPIs/libRDTAPIs/libTUTKGlobalAPIs`)
- the Android bionic runtime
- `EBO_LICENSE` (the app-wide Kalay license)
- `ioctl9930.bin` (the "start streaming" command)

## The only things unique to *your* robot

Four strings that authenticate to your specific unit — these cannot be borrowed:

| `.env` var | Meaning | Hooked from |
|---|---|---|
| `EBO_UID` | 20-char Kalay address (which robot) | `IOTC_Connect_ByUIDEx` |
| `EBO_AUTHKEY` | 8-char auth key | `IOTC_Connect_ByUIDEx` input struct |
| `EBO_IDENTITY` | account UUID (DTLS-PSK identity) | `avClientStartEx` in-config |
| `EBO_TOKEN` | session token/password (PSK secret) | `avClientStartEx` in-config |

## How to capture them

1. **Android with Frida.** Rooted phone or emulator with `frida-server` running
   (or repackage the app with `frida-gadget`). Install frida locally:
   `pip install frida-tools`.
2. Find the package: `frida-ps -Uai | grep -i ebo` (commonly `com.enabot.rola`).
3. Attach the hook and launch the app:
   ```
   frida -U -f com.enabot.rola -l src/extract/frida_ebo_creds.js --no-pause
   ```
4. **Open live view** in the app so it connects to the robot. The script prints
   tagged `[[EBO]]` lines with the values (and hexdumps the structs so you can
   confirm the auto-detected fields). Close the app on any other phone first —
   only one client may stream at a time.
5. Copy the values into `.env` (see `.env.example`). Done — that's the app step.

## If the Frida auto-detection is ambiguous

The script hexdumps `St_IOTCConnectInput` and `AVClientStartInConfig`. If the
tagged guesses look wrong, read the values straight from the hexdump: the UID is
the 20-char `[A-Z0-9]` run, the authKey is a nearby 8-char run, and the
identity/token are the printable fields in the avClientStartEx struct.

## Then

Populate `vendor/` (borrowed libs + bionic + ioctl blob), drop the 4 values in
`.env`, and run the bridge — on this Mac via Docker+QEMU (`--platform
linux/arm/v7`) or on a Raspberry Pi. See the upstream `docs/SETUP.md`.
