#!/usr/bin/env python3
"""
Enabot Mini — LAN discovery & fingerprinting (stdlib-only, runs on macOS as-is).

Pipeline:
  1. Derive the local /24 from the active interface.
  2. Concurrent ping sweep -> live hosts (also warms the ARP cache).
  3. Per-host ARP lookup -> MAC, vendor (OUI), random-vs-real classification.
  4. TCP connect-scan + SSH banner grab on ports of interest.
  5. Score each host on how Enabot-like it is.

Key findings baked into the heuristics (from the EBO SE teardown +
this network's own scan):
  * The device is P2P/UDP only — expect NO open TCP ports on current firmware.
  * Wi-Fi is an Espressif module -> real (non-random) OUI, vendor "Espressif".
  * A *dropbear* SSH banner, if present, is a near-certain EBO signal.

Because several Espressif IoT devices can coexist, the reliable way to pin
down the exact unit is power-cycle correlation:

    python3 src/discover.py --snapshot before.json      # Enabot powered ON
    # ... unplug / power off the Enabot, wait ~20s ...
    python3 src/discover.py --snapshot after.json       # Enabot powered OFF
    python3 src/discover.py --diff before.json after.json

The host that disappears is the Enabot.

Usage:
    python3 src/discover.py                      # scan + report
    python3 src/discover.py --snapshot out.json  # scan + save snapshot
    python3 src/discover.py --diff a.json b.json  # correlate two snapshots
    python3 src/discover.py --host 192.168.68.55 # fingerprint one host
    python3 src/discover.py --no-online          # skip online OUI lookup
"""
from __future__ import annotations

import argparse
import ipaddress
import json
import re
import socket
import subprocess
import sys
import time
import urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CAP = ROOT / "captures"
OUI_CACHE = CAP / "oui_cache.json"

TCP_PORTS = {
    22: "ssh (dropbear on EBO)", 23: "telnet", 80: "http", 443: "https",
    554: "rtsp", 8000: "http-alt", 8080: "http-alt", 8443: "https-alt",
    8554: "rtsp-alt", 34567: "dvrip (cheap cams)", 49152: "upnp/onvif",
}

# Vendors that strongly suggest a generic IoT device (Enabot-class candidate).
IOT_VENDORS = ("espressif", "realtek", "ingenic", "hangzhou", "shenzhen",
               "tuya", "hi-flying", "mediatek")
# Vendors that are almost certainly NOT the Enabot (rule out fast).
KNOWN_NON_EBO = ("sonos", "bose", "apple", "tp-link", "google", "amazon",
                 "samsung", "sonoff", "philips", "roku", "ubiquiti")

# Small offline OUI seed so the tool is useful with no network.
OUI_SEED = {
    "84:fc:e6": "Espressif Inc.", "40:4c:ca": "Espressif Inc.",
    "48:22:54": "TP-Link Systems Inc", "4c:87:5d": "Bose Corporation",
    "54:2a:1b": "Sonos, Inc.", "40:ed:cf": "Apple, Inc.",
}


def sh(cmd: list[str], timeout: float = 6.0) -> str:
    try:
        return subprocess.run(cmd, capture_output=True, text=True,
                              timeout=timeout).stdout
    except Exception:
        return ""


def local_subnet() -> "ipaddress.IPv4Network | None":
    iface = ""
    m = re.search(r"interface:\s*(\S+)", sh(["route", "-n", "get", "default"]))
    if m:
        iface = m.group(1)
    ip = sh(["ipconfig", "getifaddr", iface]).strip() if iface else ""
    if not ip:
        try:
            s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            s.connect(("8.8.8.8", 80))
            ip, _ = s.getsockname(), s.close()
            ip = ip[0]
        except Exception:
            return None
    return ipaddress.ip_network(ip + "/24", strict=False) if ip else None


def ping(host: str) -> bool:
    return subprocess.run(
        ["ping", "-c", "1", "-W", "500", "-t", "1", host],
        capture_output=True, text=True).returncode == 0


def normalize_mac(mac: str) -> str:
    return ":".join(p.zfill(2).lower() for p in mac.split(":"))


def mac_for(host: str) -> str:
    """Fresh per-host ARP lookup (robust vs. a stale global table)."""
    out = sh(["arp", "-n", host])
    m = re.search(r"at\s+([0-9a-fA-F:]+)", out)
    if m and "incomplete" not in out:
        return normalize_mac(m.group(1))
    return ""


def is_random_mac(mac: str) -> bool:
    """Locally-administered bit set -> randomized (phone/laptop privacy MAC)."""
    if not mac or ":" not in mac:
        return False
    try:
        return bool(int(mac.split(":")[0], 16) & 0x02)
    except ValueError:
        return False


_oui_cache: dict = {}


def load_oui_cache() -> dict:
    global _oui_cache
    if _oui_cache:
        return _oui_cache
    _oui_cache = dict(OUI_SEED)
    if OUI_CACHE.exists():
        try:
            _oui_cache.update(json.loads(OUI_CACHE.read_text()))
        except Exception:
            pass
    return _oui_cache


def save_oui_cache() -> None:
    CAP.mkdir(parents=True, exist_ok=True)
    OUI_CACHE.write_text(json.dumps(_oui_cache, indent=2))


def vendor_for(mac: str, online: bool = True) -> str:
    """OUI -> vendor. Sends only the 3-byte prefix (vendor), never the full MAC."""
    if not mac:
        return ""
    oui = ":".join(mac.split(":")[:3])
    cache = load_oui_cache()
    if oui in cache:
        return cache[oui]
    if is_random_mac(mac):
        cache[oui] = "(randomized MAC)"
        return cache[oui]
    if not online:
        return ""
    try:
        req = urllib.request.Request(
            f"https://api.macvendors.com/{oui}",
            headers={"User-Agent": "enabot-discover/1.0"})
        v = urllib.request.urlopen(req, timeout=6).read().decode().strip()
        cache[oui] = v
        return v
    except Exception:
        return ""


def tcp_probe(host: str, port: int, timeout: float = 0.8):
    try:
        with socket.create_connection((host, port), timeout=timeout) as s:
            s.settimeout(timeout)
            try:
                banner = s.recv(256).decode("latin-1", "replace").strip()
            except Exception:
                banner = ""
            return True, banner
    except Exception:
        return False, ""


def score_host(fp: dict):
    score, why = 0, []
    vlow = (fp.get("vendor") or "").lower()
    for p in fp.get("tcp_open", []):
        if p["port"] == 22 and "dropbear" in p["banner"].lower():
            score += 60
            why.append("dropbear SSH (near-certain EBO)")
        elif p["port"] in (554, 8554) or "rtsp" in p["banner"].lower():
            score += 5
            why.append("RTSP-ish port (EBO normally has none — investigate)")
    if any(v in vlow for v in KNOWN_NON_EBO):
        score -= 40
        why.append(f"vendor {fp['vendor']} — not an Enabot")
    elif any(v in vlow for v in IOT_VENDORS):
        score += 35
        why.append(f"IoT vendor {fp['vendor']} (Enabot-class module)")
    if fp.get("random_mac"):
        score -= 30
        why.append("randomized MAC — phone/laptop, not IoT")
    # Classic P2P-cam shape: real IoT MAC, no TCP surface at all.
    if not fp.get("tcp_open") and any(v in vlow for v in IOT_VENDORS):
        score += 15
        why.append("no open TCP ports (pure-UDP P2P shape)")
    return score, why


def fingerprint(host: str, online: bool = True) -> dict:
    mac = mac_for(host)
    fp = {"ip": host, "mac": mac, "random_mac": is_random_mac(mac),
          "vendor": vendor_for(mac, online)}
    tcp_open = []
    for port, label in TCP_PORTS.items():
        ok, banner = tcp_probe(host, port)
        if ok:
            tcp_open.append({"port": port, "label": label, "banner": banner})
    fp["tcp_open"] = tcp_open
    fp["score"], fp["why"] = score_host(fp)
    return fp


def sweep(cidr, workers=64) -> list:
    hosts = [str(h) for h in cidr.hosts()]
    print(f"[*] Ping-sweeping {cidr} ({len(hosts)} hosts)…", flush=True)
    live = []
    with ThreadPoolExecutor(max_workers=workers) as ex:
        futs = {ex.submit(ping, h): h for h in hosts}
        for f in as_completed(futs):
            if f.result():
                live.append(futs[f])
    live.sort(key=lambda ip: tuple(int(o) for o in ip.split(".")))
    print(f"[*] {len(live)} host(s) up.", flush=True)
    return live


def scan(cidr, online=True) -> list:
    live = sweep(cidr)
    print("[*] Fingerprinting…", flush=True)
    results = []
    with ThreadPoolExecutor(max_workers=16) as ex:
        futs = {ex.submit(fingerprint, h, online): h for h in live}
        for f in as_completed(futs):
            results.append(f.result())
    save_oui_cache()
    results.sort(key=lambda r: r["score"], reverse=True)
    return results


def print_report(results: list) -> None:
    print("\n" + "=" * 70)
    print("DISCOVERY RESULTS (most Enabot-like first)")
    print("=" * 70)
    for r in results:
        tag = ("  <== LIKELY ENABOT" if r["score"] >= 60 else
               "  <- candidate" if r["score"] >= 30 else "")
        vend = r["vendor"] or "?"
        print(f"\n{r['ip']:<16} {r['mac'] or '?':<18} {vend:<22} "
              f"score={r['score']}{tag}")
        for p in r["tcp_open"]:
            b = f'  “{p["banner"]}”' if p["banner"] else ""
            print(f"    tcp/{p['port']:<5} {p['label']}{b}")
        for w in r["why"]:
            print(f"      · {w}")
    if not results:
        print("\n(no hosts)")


def cmd_diff(a_path: str, b_path: str) -> int:
    a = {r["ip"]: r for r in json.loads(Path(a_path).read_text())["results"]}
    b = {r["ip"]: r for r in json.loads(Path(b_path).read_text())["results"]}
    gone = [a[ip] for ip in a if ip not in b]      # in A, not in B
    new = [b[ip] for ip in b if ip not in a]
    print("=" * 70)
    print(f"DIFF  {Path(a_path).name}  ->  {Path(b_path).name}")
    print("=" * 70)
    print("\nDISAPPEARED (present in first, gone in second):")
    if gone:
        for r in sorted(gone, key=lambda r: r["score"], reverse=True):
            star = "  <== if you powered OFF the Enabot, THIS IS IT" if any(
                v in (r["vendor"] or "").lower() for v in IOT_VENDORS) else ""
            print(f"  {r['ip']:<16} {r['mac'] or '?':<18} "
                  f"{r['vendor'] or '?'}{star}")
    else:
        print("  (none)")
    print("\nAPPEARED (new in second):")
    for r in new:
        print(f"  {r['ip']:<16} {r['mac'] or '?':<18} {r['vendor'] or '?'}")
    if not new:
        print("  (none)")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="Enabot Mini LAN discovery")
    ap.add_argument("--cidr")
    ap.add_argument("--host")
    ap.add_argument("--snapshot", help="Scan and save snapshot to this path")
    ap.add_argument("--diff", nargs=2, metavar=("BEFORE", "AFTER"))
    ap.add_argument("--no-online", action="store_true",
                    help="Skip online OUI vendor lookup")
    args = ap.parse_args()
    online = not args.no_online

    if args.diff:
        return cmd_diff(*args.diff)

    if args.host:
        results = [fingerprint(args.host, online)]
        save_oui_cache()
    else:
        cidr = (ipaddress.ip_network(args.cidr, strict=False)
                if args.cidr else local_subnet())
        if not cidr:
            print("[!] Could not determine subnet; pass --cidr.", file=sys.stderr)
            return 2
        results = scan(cidr, online)

    print_report(results)
    out = args.snapshot or str(CAP / "discovery.json")
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    Path(out).write_text(json.dumps(
        {"ts": int(time.time()), "results": results}, indent=2))
    print(f"\n[*] Snapshot written to {out}")
    if args.snapshot:
        print("[*] Power-cycle the Enabot, re-snapshot, then run --diff.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
