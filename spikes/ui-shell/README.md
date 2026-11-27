# Native UI shell spike

Status: spike for the Club Voice desktop client, Windows first. Three toolkits
render the same main window from the same fake data so they can be compared on
equal terms before one is chosen (TECHNICAL_PLAN, gate 1). No networking, no
audio, no microphone access.

| Crate | Toolkit | Model |
| --- | --- | --- |
| `model` | none | v5 snapshot types, tree flattening, permissions, local state |
| `egui-shell` | egui/eframe 0.37 (glow) | immediate mode |
| `slint-shell` | Slint 1.19 | declarative, retained |
| `iced-shell` | iced 0.14 | Elm-style, retained |

The window follows the layout agreed in Issue #1: room tree on the left with
occupants under their rooms, room title and chat in the center, a call bar at
the bottom, and a per-participant popover with volume, local mute and
moderation actions when the viewer's role allows them.

Fake data is `fixtures/club-demo.json`, a v5 `list` reply that extends the
contract example in `fixtures/v5-contract-example.json`. The viewer is a
moderator in Team A. Cyrillic names exercise font fallback.

## Running

```
cv-shell-egui.exe  [--scenario idle|speaking] [--exit-after SECONDS]
cv-shell-slint.exe [--scenario idle|speaking] [--exit-after SECONDS]
cv-shell-iced.exe  [--scenario idle|speaking] [--exit-after SECONDS]
```

- `idle` (default): nothing changes after the first frame.
- `speaking`: speaking rings change every 250 ms.
- `--exit-after`: closes the window and prints one JSON line with the frame
  count (egui: `ui` calls, Slint: rendered frames, iced: `view` rebuilds) and
  the time to the first frame. Redirect stdout to capture it.

Things to try: collapse and expand rooms, click a participant to open the
popover, change volume, mute locally, send a chat line, toggle mic and sound.
Buttons without behavior (Saved servers, Devices, Move, Disconnect, Leave)
are placeholders.

## Building

```
cargo test -p cv-ui-model
cargo build --release --workspace                                  # native
cargo build --release --workspace --target x86_64-pc-windows-gnu   # from Linux, needs mingw-w64
```

This is a standalone workspace; it does not touch the product workspace. The
release Windows binaries import only system DLLs (checked with
`x86_64-w64-mingw32-objdump -p`).

## Findings so far

- Slint's generated code carries `#[allow(unsafe_code)]`, which a crate-level
  `unsafe_code = "forbid"` rejects (E0453). The Slint shell uses `deny`. If
  Slint is chosen, the UI crate needs `deny`, or the product's `forbid` has to
  be relaxed for that crate only.
- egui's default fonts have no arrow, lock or dot glyphs; the egui shell paints
  them or uses text. Slint and iced picked them up from system fonts on Linux.
- Release binary sizes, Windows GNU target, stripped, thin LTO: egui 6.2 MB,
  iced 9.6 MB, Slint 13.9 MB.
- Measurements of memory, CPU, idle redraws and minimized behavior on Windows
  are pending.
