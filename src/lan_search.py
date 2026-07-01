#!/usr/bin/env python3
"""
Enabot Mini — PPPP/CS2 LAN broadcast discovery.

The EBO firmware carries the string "Charlie is the designer of P2P!!", which
is the init/XOR signature of CS2 Network's **PPPP** P2P stack (the same family
the cam-reverse project talks to). PPPP devices answer a LAN broadcast search:

    We broadcast   MSG_LAN_SEARCH  = F1 30 00 00
    A camera replies MSG_PUNCH_PKT = F1 41 00 <len> <DID...>   (contains device ID)

So we spray the search to the broadcast address on the usual PPPP ports and
listen for anyone who answers. Whoever replies is a P2P camera — almost
certainly the EBO — and the reply embeds its Device ID (needed to connect).

This is non-invasive: a single broadcast + passive listen, no ARP spoofing,
no power-cycling.

Usage:
    python3 src/lan_search.py                 # default ports, 6s listen
    python3 src/lan_search.py --secs 10
    python3 src/lan_search.py --bcast 192.168.68.255
"""
from __future__ import annotations

import argparse
import json
import socket
import struct
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CAP = ROOT / "captures"

# PPPP message magics (CS2 Network).
MSG_LAN_SEARCH = bytes([0xF1, 0x30, 0x00, 0x00])
# A couple of other search shapes seen across PPPP forks, in case the Mini
# uses a variant. All harmless broadcast probes.
EXTRA_PROBES = [
    bytes([0xF1, 0x30, 0x00, 0x00]),
    bytes([0xF1, 0x36, 0x00, 0x00]),      # some forks
    b"\x2c\x57\x39\x30",                   # alt magic seen in the wild
]

# Ports PPPP/CS2 cameras commonly listen on for LAN search.
PPPP_PORTS = [32108, 32761, 32100, 10000, 10240, 12345]


def parse_reply(data: bytes) -> str:
    """Best-effort extraction of a printable Device ID from a PPPP reply."""
    # PPPP punch replies carry the DID as an ASCII-ish blob (prefix + serial +
    # suffix). Pull the longest printable run as a hint.
    runs, cur = [], []
    for b in data:
        if 0x20 <= b < 0x7F:
            cur.append(chr(b))
        else:
            if len(cur) >= 4:
                runs.append("".join(cur))
            cur = []
    if len(cur) >= 4:
        runs.append("".join(cur))
    return max(runs, key=len) if runs else ""


def lan_search(bcasts: list, ports: list, secs: float) -> list:
    s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    s.setsockopt(socket.SOL_SOCKET, socket.SO_BROADCAST, 1)
    s.bind(("0.0.0.0", 0))
    s.settimeout(0.5)

    print(f"[*] Broadcasting PPPP LAN-search to {bcasts} on ports {ports}")
    for bc in bcasts:
        for port in ports:
            for probe in EXTRA_PROBES:
                try:
                    s.sendto(probe, (bc, port))
                except Exception:
                    pass

    print(f"[*] Listening {secs:.0f}s for replies (re-broadcasting each sec)…")
    replies = []
    seen = set()
    deadline = time.time() + secs
    last_bcast = 0.0
    while time.time() < deadline:
        # Re-broadcast roughly once a second to catch devices mid-boot.
        if time.time() - last_bcast > 1.0:
            for bc in bcasts:
                for port in ports:
                    try:
                        s.sendto(MSG_LAN_SEARCH, (bc, port))
                    except Exception:
                        pass
            last_bcast = time.time()
        try:
            data, addr = s.recvfrom(2048)
        except socket.timeout:
            continue
        key = (addr[0], addr[1], data[:8])
        if key in seen:
            continue
        seen.add(key)
        did = parse_reply(data)
        rec = {"ip": addr[0], "port": addr[1], "len": len(data),
               "head": data[:16].hex(), "device_id_guess": did}
        replies.append(rec)
        print(f"  <== REPLY from {addr[0]}:{addr[1]}  {len(data)}B  "
              f"head={data[:8].hex()}  id?={did!r}")
    s.close()
    return replies


def main() -> int:
    ap = argparse.ArgumentParser(description="PPPP LAN broadcast discovery")
    ap.add_argument("--bcast", action="append",
                    help="broadcast address(es); default 255.255.255.255 + subnet")
    ap.add_argument("--ports", default=",".join(map(str, PPPP_PORTS)))
    ap.add_argument("--secs", type=float, default=6.0)
    args = ap.parse_args()

    bcasts = args.bcast or ["255.255.255.255", "192.168.68.255"]
    ports = [int(p) for p in args.ports.split(",") if p.strip()]

    replies = lan_search(bcasts, ports, args.secs)

    print("\n" + "=" * 60)
    if replies:
        print(f"FOUND {len(replies)} P2P responder(s):")
        for r in replies:
            print(f"  {r['ip']}:{r['port']}  id?={r['device_id_guess']!r}  "
                  f"head={r['head']}")
        print("\n=> The responder is your EBO. Device-ID guess above feeds the "
              "connect step.")
    else:
        print("No PPPP replies. Either the Mini uses a different search magic/"
              "port, or it's not answering LAN search. Next step: passive "
              "broadcast capture (src/sniff.py) to watch what it actually emits.")
    CAP.mkdir(parents=True, exist_ok=True)
    (CAP / "lan_search.json").write_text(json.dumps(
        {"ts": int(time.time()), "replies": replies}, indent=2))
    print(f"[*] Written to {CAP / 'lan_search.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
