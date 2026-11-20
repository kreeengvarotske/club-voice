"""Build the release examples with media-smoke, then run from the repository root."""
from contextlib import ExitStack
from pathlib import Path
from subprocess import PIPE, Popen
import os
import socket
import struct
from queue import Queue
from threading import Thread


def launch(stack, name, *args):
    root = Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "release/examples"
    exe = root / (name + (".exe" if os.name == "nt" else ""))
    child = Popen([exe, *args], stdout=PIPE, text=True)
    stack.callback(child.wait, timeout=5)
    stack.callback(child.kill)
    return child


def ready_line(child):
    result = Queue(maxsize=1)
    Thread(target=lambda: result.put(child.stdout.readline().strip()), daemon=True).start()
    return result.get(timeout=5)


for duration in ("10", "20"):
    with ExitStack() as stack:
        server = launch(stack, "local_control")
        address = ready_line(server)
        receiver = launch(stack, "synthetic_player", address, "receive", duration)
        if ready_line(receiver) != "READY":
            raise RuntimeError("receiver failed to join")
        # An unadmitted sender must not inject a frame into the receiver's decoder.
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as outsider:
            host, port = address.rsplit(":", 1)
            outsider.connect((host, int(port)))
            outsider.settimeout(3)
            outsider.send(struct.pack("!4sBIQ", b"CVM1", int(duration), 999, 0) + b"x")
            outsider.send(b"CV1 PING")
            if outsider.recv(32) != b"JOIN_REQUIRED":
                raise RuntimeError("unadmitted peer received an unexpected response")
        sender = launch(stack, "synthetic_player", address, "send", duration)
        sender.communicate(timeout=10)
        report, _ = receiver.communicate(timeout=10)
        if sender.returncode or receiver.returncode:
            raise RuntimeError(f"media probe failed at {duration} ms")
        print(report.strip())
