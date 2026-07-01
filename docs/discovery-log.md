# Discovery log — finding & talking to the Enabot Mini on the LAN

_Run 2026-07-01 from this Mac (wired en0, 192.168.68.0/24)._

## What we ran
- `src/discover.py` — ping sweep + ARP + OUI vendor + TCP fingerprint of the /24.
- `src/probe.py` — active connected-UDP probing (ICMP-refused detection).
- `src/lan_search.py` — PPPP/CS2 broadcast LAN-search.
- Wide + slow UDP port scans across the Espressif candidates.

## What we found
- **17 hosts up.** Vendors resolved by OUI:
  - `.1` TP-Link (gateway), `.51` Bose, `.53/.59` Sonos, `.60` Apple, seven
    randomized-MAC hosts (phones/laptops).
  - **Five Espressif hosts — the only Enabot-class candidates:**
    `192.168.68.50, .54, .55, .56, .57`.
- **All five expose ZERO TCP ports** (no dropbear/SSH — current firmware
  dropped what debugmen.dev found on the 2022 EBO SE).
- **All five refuse every P2P/control UDP port** (32100/32108/32761 + a 230-port
  sweep), confirmed with slow repeated probes to defeat ICMP rate-limiting.
  The "open" ports seen under a fast scan were rate-limiting artifacts, not
  real listeners.
- **No PPPP LAN-search reply**, no mDNS/reverse-DNS hostnames.

## Conclusion
The EBO is **outbound-only** — the standard CS2/TUTK P2P camera model. It
connects *out* to cloud relay/supernode servers; even the phone app reaches it
via those servers doing NAT hole-punching. It never listens on a stable local
port, so **inbound LAN probing cannot identify or reach it** — proven, not
assumed.

## Therefore, the paths that actually work
1. **Device UID/serial (fastest to a real connection).** The CS2/PPPP UID is the
   key to connecting via the cloud. It's usually on a sticker on the device / box,
   or in the app under Device Info. With it, a CS2/PPPP client (adapt
   `cam-reverse`) can establish the session.
2. **Intercept the app (best for full protocol + auth).** Decompile the APK
   (JADX) and MITM the HTTPS cloud API with Frida unpinning → login flow, device
   list (contains the UID), and the P2P bootstrap. Requires an Android phone.
3. **ARP-spoof MITM the EBO (headless, no phone).** Become MITM between the EBO
   and the router (debugmen's method) to capture its cloud handshake + UID.
   Invasive, needs root — only on your own hardware.
4. **Router client list (fastest to just ID the local IP).** The TP-Link at
   `192.168.68.1` should list the device by name; match its MAC to the five above.

## Open item
Which of `.50/.54/.55/.57/.56` is the EBO is still unconfirmed — it can't be
determined by probing. Router list, or correlating the MAC/UID from path 1–3,
will settle it. (`.54` and `.56` were the quietest — only mDNS — but that's weak.)
