# enabot_claude

Experiments connecting to an **Enabot Mini** (Enabot EBO series companion robot / camera)
programmatically — fetching the video stream, controlling movement, and exploring its API.

## Status

Early research. A deep-research report on connectivity options (official app/cloud API,
Tuya vs. proprietary platform, RTSP/ONVIF/local streaming, reverse-engineering the app,
existing open-source integrations, protocols/ports, auth) is being gathered.

## Layout

- `docs/` — research notes and findings
- `src/` — client / integration code (TBD once the protocol is understood)

## Safety note

This repo may contain traffic captures and decompilation notes for a device you own.
Never commit credentials, device tokens, or captured auth material — see `.gitignore`.
