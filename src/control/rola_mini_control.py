#!/usr/bin/env python3
"""Control a logged-in ROLA Mini session through the patched Android app.

The phone app remains the authenticated transport. This CLI only attaches to
Frida Gadget and asks the already-connected app to send its normal Mini control
messages.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

import frida


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_AGENT = ROOT / "src" / "control" / "frida_rola_mini_control.js"


def ensure_forward(serial: str | None, port: int) -> None:
    if not serial:
        return
    subprocess.run(
        ["adb", "-s", serial, "forward", f"tcp:{port}", f"tcp:{port}"],
        check=True,
    )


def attach(host: str, process: str, agent_path: Path):
    device = frida.get_device_manager().add_remote_device(host)
    session = device.attach(process)
    script = session.create_script(agent_path.read_text())

    def on_message(message, data):
        if message.get("type") == "error":
            print(message.get("stack") or message, file=sys.stderr)
        elif message.get("type") == "send":
            print(message.get("payload"), file=sys.stderr)

    script.on("message", on_message)
    script.load()
    return session, script, script.exports_sync


def print_json(value) -> None:
    print(json.dumps(value, indent=2, sort_keys=True))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Control ROLA Mini via Frida Gadget")
    parser.add_argument("--host", default="127.0.0.1:27042")
    parser.add_argument("--process", default="Gadget")
    parser.add_argument("--agent", type=Path, default=DEFAULT_AGENT)
    parser.add_argument("--adb-serial", help="Run adb forward tcp:27042 tcp:27042 first")

    sub = parser.add_subparsers(dest="command", required=True)
    sub.add_parser("status", help="Show whether a live model/session is available")
    sub.add_parser("stop", help="Send neutral joystick command")

    move = sub.add_parser("move", help="Send one joystick command")
    move.add_argument("--ly", type=int, required=True, help="Forward/back axis, -100..100")
    move.add_argument("--rx", type=int, required=True, help="Turn axis, -100..100")
    move.add_argument("--buttons", type=int, default=1)

    pulse = sub.add_parser("pulse", help="Move briefly, then stop")
    pulse.add_argument("--ly", type=int, required=True, help="Forward/back axis, -100..100")
    pulse.add_argument("--rx", type=int, required=True, help="Turn axis, -100..100")
    pulse.add_argument("--buttons", type=int, default=1)
    pulse.add_argument("--ms", type=int, default=250)

    args = parser.parse_args(argv)

    port = int(args.host.rsplit(":", 1)[1])
    ensure_forward(args.adb_serial, port)

    session = None
    try:
      session, _script, api = attach(args.host, args.process, args.agent)
      if args.command == "status":
          print_json(api.status())
      elif args.command == "stop":
          print_json(api.stop())
      elif args.command == "move":
          print_json(api.move(args.ly, args.rx, args.buttons))
      elif args.command == "pulse":
          print_json(api.move(args.ly, args.rx, args.buttons))
          time.sleep(max(args.ms, 0) / 1000)
          print_json(api.stop())
      else:
          parser.error(f"unknown command: {args.command}")
    finally:
      if session is not None:
          session.detach()

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
