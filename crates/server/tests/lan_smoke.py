"""Exercise the private-interface pilot on one Windows or Linux PC."""
import argparse
from contextlib import ExitStack
from pathlib import Path
import ipaddress
import os
from queue import Queue
import socket
import struct
from subprocess import PIPE, Popen
from threading import Thread


def launch(stack, name, args, lines, *, seconds=None):
    root = Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "release/examples"
    executable = root / (name + (".exe" if os.name == "nt" else ""))
    env = dict(os.environ)
    if seconds is not None:
        env["CLUB_VOICE_PROBE_SECONDS"] = str(seconds)
    child = Popen([executable, *args], stdin=PIPE, stdout=PIPE, stderr=PIPE, text=True, env=env)
    stack.callback(child.wait, timeout=5)
    stack.callback(child.kill)
    child.stdin.write("\n".join(lines) + "\n")
    child.stdin.flush()
    return child


def read_ready(child):
    line = Queue(maxsize=1)
    Thread(target=lambda: line.put(child.stdout.readline()), daemon=True).start()
    return line.get(timeout=5).strip()


def run(ip, check_client):
    password = "lan-smoke-only"
    with ExitStack() as stack:
        relay = launch(stack, "local_control", ["--lan"], [f"{ip}:0", password])
        ready = read_ready(relay)
        if "LAN pilot listening on " not in ready:
            raise RuntimeError(f"LAN relay did not start: {ready}")
        address = ready.split("LAN pilot listening on ", 1)[1]
        args = ["--join-lan", "--tone-peer"]
        rejected = launch(stack, "headset_player", args, [address, "Denied", "bad-password"])
        _, error = rejected.communicate(timeout=5)
        if rejected.returncode == 0 or "DENIED" not in error:
            raise RuntimeError(f"wrong password was not rejected: {error}")
        clients = [
            launch(stack, "headset_player", args, [address, name, password])
            for name in ("Ada", "Lin")
        ]
        for client in clients:
            report, error = client.communicate(timeout=10)
            if client.returncode or "arrival_to_render_p95_ms=" not in report:
                raise RuntimeError(f"LAN duplex failed: {report}\n{error}")
            print(report.strip())
        if check_client:
            companion = launch(
                stack, "headset_player", args, [address, "Companion", password], seconds=12
            )
            if not read_ready(companion).endswith("READY"):
                raise RuntimeError("test companion did not join")
            check = launch(
                stack, "headset_player", ["--check-lan"], [address, password], seconds=3
            )
            report, error = check.communicate(timeout=12)
            if check.returncode or "PASS: LAN check complete" not in report:
                raise RuntimeError(f"combined LAN check failed: {report}\n{error}")
            companion_report, error = companion.communicate(timeout=10)
            if companion.returncode or "streams_heard=2 " not in companion_report:
                raise RuntimeError(f"companion did not hear both sessions: {companion_report}\n{error}")
            print("Combined LAN check:", report.strip())

    # Echo real Opus from a second session with deliberately incompatible wall clocks.
    with ExitStack() as stack:
        relay = stack.enter_context(socket.socket(socket.AF_INET, socket.SOCK_DGRAM))
        relay.bind((str(ip), 0))
        relay.settimeout(5)
        address = f"{ip}:{relay.getsockname()[1]}"
        client = launch(stack, "headset_player", args, [address, "Clock test", password])
        request, endpoint = relay.recvfrom(256)
        if not request.startswith(b"CV2 JOIN\0"):
            raise RuntimeError("expected voice admission")
        relay.sendto(struct.pack("!4sQ", b"CV2A", 1), endpoint)
        while True:
            packet, sender = relay.recvfrom(1301)
            if sender != endpoint:
                continue
            if packet.startswith(b"CV2L"):
                break
            if packet.startswith(b"CVM2"):
                sequence = struct.unpack_from("!I", packet, 5)[0]
                stamp = 0 if sequence % 2 == 0 else 2**64 - 1
                relay.sendto(packet[:9] + struct.pack("!QQ", stamp, 2) + packet[25:], endpoint)
        report, error = client.communicate(timeout=5)
        if client.returncode or "arrival_to_render_p95_ms=" not in report:
            raise RuntimeError(f"LAN packet timestamps affected playback: {report}\n{error}")
        print("Clock independence:", report.strip())


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("ip", type=ipaddress.IPv4Address, help="private IPv4 on this machine")
    parser.add_argument("--check-client", action="store_true", help="also exercise the combined device and network check")
    args = parser.parse_args()
    run(args.ip, args.check_client)
