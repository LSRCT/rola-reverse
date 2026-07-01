# Connecting to an Enabot Mini (EBO series) Programmatically — Research

_Compiled 2026-07-01 via a deep-research pass (5 search angles, 15 sources fetched, 25 claims adversarially verified, 23 confirmed)._

> **Model-scope caveat, read first:** Nearly all primary reverse-engineering below targets the **EBO SE**, not the specifically-named **Mini**. Packet structures, codec (H.264 vs H.265), ports, and credentials may differ across models/firmware. Treat everything as a strong starting hypothesis to **re-validate on your actual Mini**. Also: the debugmen writeups are 2022–2023 (firmware `ipc20211223`); recovered passwords and the OTA-interception method may be hardened in current firmware. Only do this on hardware you own — credential cracking, ARP-spoofing, MITM, and APK decompilation carry ToS/legal implications.

## TL;DR

- **Platform:** ThroughTek **TUTK / Kalay P2P** — *not* Tuya. Connection is keyed on the device's **TUTK ID (serial number)**.
- **Open standards:** **No RTSP, no ONVIF, no local standard stream.** It's a proprietary binary UDP protocol.
- **Transport:** Custom **MAVLink-based binary protocol over UDP (port 32761 on the SE)**, wrapped in a custom XOR + scramble scheme (XOR key fragment: `Charlie is the designer of P2P!!`, a known ThroughTek/IOTC signature).
- **Media:** Video is **H.264/H.265** (reconstructable with FFmpeg); audio is **G.711 @ 8kHz**. Media *is* accessible once you parse the UDP protocol.
- **Control:** Movement = crafted UDP packets with **IEEE float32** motor values, branch IDs (e.g. `0xbeca`), and a custom firmware CRC.
- **Best off-the-shelf path:** [`lilium360/ebo-se-lan-bridge`](https://github.com/lilium360/ebo-se-lan-bridge) — runs the TUTK/Kalay client on a Raspberry Pi for phone-free, cloud-free local control + live H.265 video (WebRTC/RTSP out) + joystick web panel + battery/diagnostics over MQTT to Home Assistant. **New, single-author, EBO SE.**

---

## The three practical paths

### Path A — Use the community bridge (fastest, least work)
[`lilium360/ebo-se-lan-bridge`](https://github.com/lilium360/ebo-se-lan-bridge) (Python, MIT, created 2026-06-20). Runs the TUTK/Kalay client on a Raspberry Pi and gives you:
- Local, cloud-free, phone-free control
- Live **H.265 1080p30** video re-exposed as **WebRTC/RTSP**, G.711 audio (listen-only)
- A **joystick web panel**
- **Battery & diagnostic entities over MQTT** → Home Assistant

**Reality check:** brand-new, ~3 stars, one author, built and tested against the **EBO SE**. There is **no official Home Assistant integration** — only this project and an open feature request. First step for the Mini: clone it, point it at your device, and see how much of the protocol matches.

### Path B — Reverse-engineer the local UDP protocol directly
This is the [debugmen.dev Enabot series](https://debugmen.dev/hardware-series/2022/08/01/enabot_series_part_2.html) approach (Parts 1–3), independently corroborated by an HA-forum dev who reversed the ROLA Android app.
1. Put the device on a laptop/PC hotspot; capture UDP traffic in **Wireshark**.
2. Packets are XOR'd with `Charlie is the designer of P2P!!` then scrambled (`ReverseTransCodePartial` / "charlie_scramble" in `libIOTCAPIs.so`). Reverse the scramble to get plaintext frames.
3. **Video:** ~1122-byte packets; P-frame header `0x0141`, I-frame `0x01d7`. Concatenate payloads → feed to **FFmpeg**.
4. **Audio:** G.711, 8kHz, ~32 packets/sec of `0x100` bytes → playable via pyaudio.
5. **Movement:** send motor packets — IEEE float32 values (e.g. `0x3f97a68b` = +1.185 right, `0xbfd4a775` = −1.661 left), branch selector `0xbeca` (the `0xca` byte picks the button/skill), `mode==0xc8` triggers self-check. Packets need a **custom CRC** (state machine; observed final CRC bytes `[15 27]`).
6. Initial connection needs the **TUTK ID (serial)**; debugmen obtained it via **ARP spoofing**.

### Path C — Run the official TUTK/Kalay client
Instead of reversing the wire format, drive ThroughTek's IOTC/AVAPI client library directly using the device's TUTK ID/serial to establish the session, then pull A/V and send IO_CTRL commands. This is essentially what the bridge (Path A) does under the hood. Note: TUTK/Kalay is **not** fully decentralized — it typically still uses cloud servers for NAT-traversal/discovery even though media is peer-to-peer.

---

## Understanding the app's cloud API + auth (mobile RE)
No source captured the actual HTTPS API calls, so the backend REST API (endpoints, login/token flow, how the TUTK ID is provisioned) is an **open question**. Standard toolchain to find it yourself:
1. **Decompile the APK** with **JADX** → readable Java, exposes endpoints (obfuscation/lost var names limit clarity).
2. **MITM the HTTPS traffic**, defeating cert pinning with **Frida**: ADB → install `frida-server` on device → `pip install frida-tools` → apply [`httptoolkit/frida-interception-and-unpinning`](https://github.com/httptoolkit/frida-interception-and-unpinning) → route through a proxy (HTTP Toolkit).
   - **Caveat (verified):** these scripts do **not** reliably beat *hardened/obfuscated* pinning + certificate transparency. May need a rooted device and tailored scripts. The Enabot app specifically was not tested in the sources.

## Firmware / on-device access (EBO SE, 2021–22 firmware)
- Embedded **ARM (armv5t) Linux, kernel 4.9.84**; main process `/usr/userfs/bin//FW_EBO_C`; **dropbear SSH**.
- Root DES hash `RKVyRbEzRyync` cracked to **`fz@2019*`** (older backup hash: `helpme`) via crack.sh. → on-device shell + `gdbserver`/`gdb-multiarch` debugging.
- **Watchdog:** ~10s hardware watchdog (`ioctl 0xC0045706`) reboots on firmware halt; to debug you must kill `FW_EBO_C` and `wpa_supplicant` (both hold watchdog handles) and keep the watchdog fed.
- **Firmware acquisition:** intercept the **OTA download** over the network (device on PC hotspot + Wireshark) → yields a tar (`1640594394-ebo-se-ipc20211223.tar`) → analyze with **binwalk**. (May be TLS-protected on newer firmware.)

## Analogous prior art (proves feasibility, not direct support)
- [`DavidVentura/cam-reverse`](https://github.com/DavidVentura/cam-reverse) — reimplements the PPPP/iLnk P2P protocol, exposes MJPEG at `http://localhost:5000/`. Different protocol family (not TUTK), but proves P2P-to-HTTP bridging works.
- [`indykoning/home-assistant-p2pcam`](https://github.com/indykoning/home-assistant-p2pcam) — HA component pulling images from generic Chinese P2P cams.

---

## Open questions to resolve on the actual Mini
1. Does the **Mini** use the same TUTK/Kalay stack, UDP port 32761, and packet/CRC formats as the reversed EBO SE?
2. Exact structure + auth of the Enabot/ROLA cloud API (endpoints, login/token, TUTK ID provisioning) — no source captured live calls.
3. Can `ebo-se-lan-bridge` (or the raw UDP protocol) be adapted to the Mini? Does it work fully offline, or does pairing still need the vendor cloud for TUTK ID / NAT-traversal?
4. Do current (2025–26) firmware versions still expose root SSH with crackable creds, or has the OTA channel + passwords been hardened?

## Key sources
- **debugmen.dev Enabot series** (primary RE, EBO SE): [Part 1](https://debugmen.dev/hardware-series/2022/02/18/enabot_series_part_1.html) · [Part 2](https://debugmen.dev/hardware-series/2022/08/01/enabot_series_part_2.html) · [Part 2 debugging](https://debugmen.dev/hardware-series/2022/02/18/enabot_series_part_2_debugging.html) · [Part 3](https://debugmen.dev/hardware-series/2023/02/19/enabot_series_part_3.html)
- **HA forum thread** "Enabot EBO integration – camera with wheels": https://community.home-assistant.io/t/enabot-ebo-integration-camera-with-wheels/328355
- **Frida unpinning:** https://github.com/httptoolkit/frida-interception-and-unpinning · https://httptoolkit.com/blog/frida-certificate-pinning/ · https://httptoolkit.com/blog/android-reverse-engineering/
- **P2P bridging prior art:** https://github.com/DavidVentura/cam-reverse · https://github.com/indykoning/home-assistant-p2pcam
