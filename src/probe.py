#!/usr/bin/env python3
"""
Enabot Mini — active UDP prober.

"Talk to all of them; the one that answers like an EBO is the right one."

For each host+port we open a *connected* UDP socket and send probe payloads.
A connected UDP socket lets the OS surface ICMP port-unreachable as
ConnectionRefusedError, which is what distinguishes a listening device from
one that isn't:

    reply bytes   -> device actively answered on this port   (STRONGEST)
    timeout       -> port open|filtered (listening, ignored our probe)
    conn-refused  -> ICMP port-unreachable => nothing listening there

The EBO control channel is UDP/32761 (MAVLink-ish, per the EBO SE teardown).
An Espressif smart-plug/bulb will refuse 32761; the EBO should not.

Usage:
    python3 src/probe.py 192.168.68.50 192.168.68.54 192.168.68.55 ...
    python3 src/probe.py --ports 32761,32108,10000 <hosts...>
    python3 src/probe.py --auto        # probe all Espressif hosts from last scan
"""
from __future__ import annotations

import argparse
import json
import socket
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CAP = ROOT / "captures"

# Candidate UDP ports. 32761 = confirmed EBO SE control port. The others are
# common TUTK/Kalay + P2P-cam ports worth a poke in case the Mini differs.
DEFAULT_PORTS = [32761, 32108, 10000, 10001, 20000, 8600, 6688, 8305]

# Probe payloads to try. We don't have the real TUTK handshake, so we lead with
# harmless bytes and a couple of shapes that P2P stacks sometimes answer.
PROBES = [
    ("zeros", b"\x00\x00\x00\x00"),
    # PPPP/TUTK-family LAN-search-ish magic (speculative; safe to send).
    ("f1magic", b"\xf1\x30\x00\x00"),
    ("hello", b"HELLO"),
]

RESULT = {"reply": 3, "open|filtered": 2, "refused": 0, "error": -1}


def probe_port(host: str, port: int, timeout: float = 1.2) -> dict:
    """Return classification + any reply seen for host:port/udp."""
    best = {"host": host, "port": port, "state": "error",
            "reply_hex": "", "reply_len": 0, "which": ""}
    for name, payload in PROBES:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.connect((host, port))
            s.settimeout(timeout)
            s.send(payload)
            try:
                data = s.recv(4096)
                best.update(state="reply", reply_hex=data[:32].hex(),
                            reply_len=len(data), which=name)
                s.close()
                return best  # a real reply is decisive; stop here
            except socket.timeout:
                if RESULT["open|filtered"] > RESULT.get(best["state"], -2):
                    best["state"] = "open|filtered"
            except ConnectionRefusedError:
                if best["state"] == "error":
                    best["state"] = "refused"
            finally:
                s.close()
        except ConnectionRefusedError:
            if best["state"] == "error":
                best["state"] = "refused"
        except Exception as e:
            best["which"] = f"err:{e}"
        time.sleep(0.05)
    return best


def espressif_hosts_from_last_scan() -> list:
    f = CAP / "discovery.json"
    if not f.exists():
        return []
    data = json.loads(f.read_text())
    return [r["ip"] for r in data["results"]
            if "espressif" in (r.get("vendor") or "").lower()]


def main() -> int:
    ap = argparse.ArgumentParser(description="Enabot active UDP prober")
    ap.add_argument("hosts", nargs="*")
    ap.add_argument("--ports", help="comma-separated UDP ports",
                    default=",".join(map(str, DEFAULT_PORTS)))
    ap.add_argument("--auto", action="store_true",
                    help="probe Espressif hosts from captures/discovery.json")
    args = ap.parse_args()

    hosts = list(args.hosts)
    if args.auto:
        hosts += espressif_hosts_from_last_scan()
    hosts = sorted(set(hosts), key=lambda ip: tuple(int(o) for o in ip.split(".")))
    if not hosts:
        print("[!] No hosts. Pass IPs or use --auto after a scan.", file=sys.stderr)
        return 2
    ports = [int(p) for p in args.ports.split(",") if p.strip()]

    print(f"[*] Probing {len(hosts)} host(s) x {len(ports)} UDP port(s)…\n")
    summary = []
    for h in hosts:
        print(f"=== {h} ===")
        host_hit = False
        row = {"host": h, "ports": {}}
        for p in ports:
            r = probe_port(h, p)
            row["ports"][p] = r["state"]
            mark = ""
            if r["state"] == "reply":
                mark = f"  <== REPLY {r['reply_len']}B [{r['which']}]: {r['reply_hex']}"
                host_hit = True
            elif r["state"] == "open|filtered" and p == 32761:
                mark = "  <- 32761 listening (EBO-shaped)"
                host_hit = True
            print(f"  udp/{p:<6} {r['state']}{mark}")
        row["ebo_shaped"] = host_hit
        summary.append(row)
        print()

    print("=" * 60)
    print("SUMMARY")
    print("=" * 60)
    ranked = sorted(summary, key=lambda r: (
        any(s == "reply" for s in r["ports"].values()),
        r["ports"].get(32761) in ("reply", "open|filtered"),
    ), reverse=True)
    for r in ranked:
        c32 = r["ports"].get(32761, "-")
        replied = [p for p, s in r["ports"].items() if s == "reply"]
        note = ""
        if replied:
            note = f"REPLIED on {replied} <== TALK TO THIS ONE"
        elif c32 in ("open|filtered",):
            note = "32761 open (listening) — top EBO candidate"
        elif all(s == "refused" for s in r["ports"].values()):
            note = "all refused — not the EBO"
        print(f"  {r['host']:<16} 32761={c32:<14} {note}")

    CAP.mkdir(parents=True, exist_ok=True)
    (CAP / "probe.json").write_text(json.dumps(
        {"ts": int(time.time()), "results": summary}, indent=2))
    print(f"\n[*] Written to {CAP / 'probe.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
