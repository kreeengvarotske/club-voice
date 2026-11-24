"""Build the release examples with media-smoke, then run from the repository root."""
from contextlib import ExitStack
from pathlib import Path
from subprocess import PIPE, Popen
import os
import socket
import struct
from queue import Queue
from threading import Thread


def launch(stack, name, *args, seconds=None, stderr=None, drop_sequence=None):
    root = Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "release/examples"
    exe = root / (name + (".exe" if os.name == "nt" else ""))
    env = dict(os.environ)
    if seconds is not None:
        env["CLUB_VOICE_PROBE_SECONDS"] = str(seconds)
    if drop_sequence is not None:
        env["CLUB_VOICE_PROBE_DROP_SEQUENCE"] = str(drop_sequence)
    child = Popen([exe, *args], stdout=PIPE, stderr=stderr, text=True, env=env)
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
            outsider.send(struct.pack("!4sBIQQ", b"CVM2", int(duration), 999, 0, 1) + b"x")
            outsider.send(b"CV1 PING")
            if outsider.recv(32) != b"JOIN_REQUIRED":
                raise RuntimeError("unadmitted peer received an unexpected response")
        sender = launch(stack, "synthetic_player", address, "send", duration)
        sender.communicate(timeout=10)
        report, _ = receiver.communicate(timeout=10)
        if sender.returncode or receiver.returncode:
            raise RuntimeError(f"media probe failed at {duration} ms")
        print(report.strip())

with ExitStack() as stack:
    address = ready_line(launch(stack, "local_control"))
    peers = [launch(stack, "headset_player", address, name, "--tone-peer")
             for name in ("Ada", "Lin")]
    for peer in peers:
        report, _ = peer.communicate(timeout=10)
        if peer.returncode:
            raise RuntimeError("duplex headset simulation failed")
        print(report.strip())
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as late:
        host, port = address.rsplit(":", 1)
        late.connect((host, int(port)))
        late.settimeout(2)
        late.send(b"CV1 JOIN\x00Fresh\x00test")
        if late.recv(32) != b"LOBBY":
            raise RuntimeError("fresh player could not join after departure")

# One listener stays connected while two consecutive participants restart at sequence zero.
with ExitStack() as stack:
    address = ready_line(launch(stack, "local_control"))
    listener = launch(stack, "headset_player", address, "Listener", "--tone-peer", seconds=9)
    if ready_line(listener) != "READY":
        raise RuntimeError("listener failed to join")
    for _ in range(2):
        peer = launch(stack, "headset_player", address, "Same display name", "--tone-peer", seconds=3)
        report, _ = peer.communicate(timeout=6)
        if peer.returncode or "streams_heard=1" not in report:
            raise RuntimeError(f"replacement peer failed: {report}")
    report, _ = listener.communicate(timeout=6)
    if listener.returncode or "streams_heard=2" not in report:
        raise RuntimeError(f"listener did not switch streams: {report}")
    print("Speaker replacement:", report.strip())

# Drop one frame at the listener. The next packet must advance the Opus state
# through one PLC frame without waiting for retransmission or growing a queue.
with ExitStack() as stack:
    address = ready_line(launch(stack, "local_control"))
    listener = launch(stack, "headset_player", address, "Loss listener", "--tone-peer",
                      seconds=6, drop_sequence=50)
    if ready_line(listener) != "READY":
        raise RuntimeError("loss listener failed to join")
    peer = launch(stack, "headset_player", address, "Loss sender", "--tone-peer", seconds=3)
    peer_report, _ = peer.communicate(timeout=6)
    listener_report, _ = listener.communicate(timeout=7)
    if peer.returncode or listener.returncode or "concealed=1" not in listener_report:
        raise RuntimeError(
            f"single-loss PLC failed: sender={peer_report} listener={listener_report}")
    print("Single-frame loss:", listener_report.strip())

# Reuse the same UDP socket. Delayed media and leave from the former session
# must have no effect on the new one, even when name and port both match.
with ExitStack() as stack:
    address = ready_line(launch(stack, "local_control"))
    host, port = address.rsplit(":", 1)

    def connect():
        peer = stack.enter_context(socket.socket(socket.AF_INET, socket.SOCK_DGRAM))
        peer.connect((host, int(port)))
        peer.settimeout(2)
        return peer

    def join(peer):
        peer.send(b"CV2 JOIN\0Test\0test")
        tag, stream = struct.unpack("!4sQ", peer.recv(32))
        if tag != b"CV2A" or not stream:
            raise RuntimeError("invalid session response")
        return stream

    def media(stream):
        return struct.pack("!4sBIQQ", b"CVM2", 20, 0, 0, stream) + b"x"

    listener, sender = connect(), connect()
    join(listener)
    old = join(sender)
    if join(sender) != old:
        raise RuntimeError("duplicate admission changed stream")
    leave = struct.pack("!4sQ", b"CV2L", old)
    sender.send(leave)
    if sender.recv(32) != b"LEFT":
        raise RuntimeError("expected leave confirmation")
    new = join(sender)
    if new <= old:
        raise RuntimeError("session identity was recycled")
    sender.send(leave)
    sender.send(media(old))
    sender.send(media(new))
    if listener.recv(64) != media(new):
        raise RuntimeError("stale datagram reached listener")
    print("Same-port rejoin rejected superseded session traffic.")

# Keep the UDP endpoint open but stop responding after admission.
with ExitStack() as stack:
    silent = stack.enter_context(socket.socket(socket.AF_INET, socket.SOCK_DGRAM))
    silent.bind(("127.0.0.1", 0))
    silent.settimeout(5)
    address = f"127.0.0.1:{silent.getsockname()[1]}"
    peer = launch(stack, "headset_player", address, "Waiting", "--tone-peer",
                  seconds=30, stderr=PIPE)
    request, endpoint = silent.recvfrom(192)
    if not request.startswith(b"CV2 JOIN\0"):
        raise RuntimeError("expected voice admission")
    silent.sendto(struct.pack("!4sQ", b"CV2A", 1), endpoint)
    _, error = peer.communicate(timeout=21)
    if peer.returncode == 0 or "server stopped responding" not in error:
        raise RuntimeError(f"silent server did not trigger heartbeat timeout: {error}")
    print("Lost server detected within the heartbeat deadline.")
