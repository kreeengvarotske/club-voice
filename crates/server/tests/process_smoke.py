"""Run from the workspace root after cargo build --workspace --examples."""
from contextlib import ExitStack
from pathlib import Path
from subprocess import PIPE, Popen
import os


def launch(stack, binary, *args):
    exe = Path("target/debug/examples") / (binary + (".exe" if os.name == "nt" else ""))
    process = Popen([exe, *args], stdin=PIPE, stdout=PIPE, text=True)
    stack.callback(process.wait, timeout=5)
    stack.callback(process.kill)
    return process


with ExitStack() as stack:
    server = launch(stack, "local_control")
    address = server.stdout.readline().strip()
    players = [launch(stack, "local_player", address, name) for name in ("Ada", "Lin")]
    for player in players:
        assert player.stdout.readline().strip() == "LOBBY"
    for player in players:
        replies, _ = player.communicate("ping\nleave\n", timeout=5)
        assert player.returncode == 0 and replies.splitlines() == ["PONG", "LEFT"]
    print("Two client processes joined, received heartbeats and left.")
