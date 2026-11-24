"""Compare two-batch overflow policies with identical synthetic playout schedules."""

from collections import deque
from dataclasses import dataclass
from math import ceil


FRAME_MS = 20
MAX_AGE_MS = 80
CAPACITY = 2


@dataclass(frozen=True)
class Frame:
    stream: int
    sequence: int
    arrival_ms: int
    concealed: bool = False


def simulate(hold_first, paused_ticks, *, loss=False, replace=False):
    batches = deque()
    pending = deque()
    discarded = []
    rendered = []
    first_after_pause = None
    active = 1
    concealed = 0
    silence = 0
    silent_ticks = 0
    expired = 0
    stale = 0
    dropped_plc = 0
    first_sequence = None
    pause = range(4, 4 + paused_ticks)
    for tick in range(16):
        time = tick * FRAME_MS
        if tick < 15 and not (loss and tick == 4):
            if replace and tick == 5:
                active += 1
            frame = Frame(active, tick, time)
            batch = (Frame(active, tick - 1, time - FRAME_MS, True), frame) if loss and tick == 5 else (frame,)
            if len(batches) == CAPACITY:
                if hold_first:
                    overflow = batch
                else:
                    overflow = batches.popleft()
                    batches.append(batch)
                discarded.append(overflow[-1].sequence)
                dropped_plc += int(overflow[0].concealed)
            else:
                batches.append(batch)
            assert len(batches) <= CAPACITY

        if tick in pause:
            continue
        while True:
            if not pending and batches:
                pending.extend(batches.popleft())
            if not pending:
                silence += 1
                silent_ticks += 1
                break
            selected = pending.popleft()
            if selected.stream != active:
                stale += 1
                continue
            age = time - selected.arrival_ms
            if age > MAX_AGE_MS:
                expired += 1
                continue
            if selected.concealed and silence:
                silence = 0
                continue
            silence = 0
            if selected.concealed:
                concealed += 1
            else:
                rendered.append((selected.sequence, age))
                if first_after_pause is None and tick >= pause.stop:
                    first_after_pause = age
                    first_sequence = selected.sequence
                assert selected.stream == active
            break

    ages = sorted(age for _, age in rendered)
    return {
        "overflow": len(discarded),
        "real": len(rendered),
        "plc": concealed,
        "first_ms": first_after_pause,
        "first_sequence": first_sequence,
        "p95_ms": ages[ceil(len(ages) * 0.95) - 1],
        "silence": silent_ticks,
        "expired": expired,
        "stale": stale,
        "overflow_plc": dropped_plc,
        "sequences": [sequence for sequence, _ in rendered],
    }


def main():
    scenarios = [
        ("steady", 0, {}),
        ("20 ms pause", 1, {}),
        ("40 ms pause", 2, {}),
        ("80 ms pause", 4, {}),
        ("80 ms pause + PLC batch", 4, {"loss": True}),
        ("40 ms pause + new stream", 2, {"replace": True}),
    ]
    print("Producer runs before the callback at each 20 ms tick. A skipped callback offers no output.")
    print("| Scenario | Keep | Overflow | Real rendered | PLC rendered | First frame after pause | First age | p95 age | Silent ticks | PLC dropped at overflow |")
    print("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |")
    for label, ticks, options in scenarios:
        for hold_first in (True, False):
            metrics = simulate(hold_first, ticks, **options)
            assert metrics["sequences"] == sorted(set(metrics["sequences"]))
            policy = "oldest" if hold_first else "newest"
            print(
                f"| {label} | {policy} | {metrics['overflow']} | {metrics['real']} | "
                f"{metrics['plc']} | {metrics['first_sequence']} | {metrics['first_ms']} ms | {metrics['p95_ms']} ms | "
                f"{metrics['silence']} | {metrics['overflow_plc']} |"
            )


if __name__ == "__main__":
    main()
