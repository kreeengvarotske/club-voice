Club Voice - test interface (UI shell spike), Windows x64
Source: branch BRANCH_PLACEHOLDER, commit SHA_PLACEHOLDER, spikes/ui-shell

What this is
  Three versions of the same Club Voice main window, built with three
  different UI libraries: egui, Slint and iced. They show FAKE data only.
  No network, no audio, the microphone is never opened. Nothing is installed.

How to run
  1. Unzip anywhere.
  2. Double-click cv-shell-egui.exe, cv-shell-slint.exe or cv-shell-iced.exe.
     Windows SmartScreen may warn because the files are unsigned:
     "More info" -> "Run anyway".
  Optional: run from cmd with  --scenario speaking  to animate who is talking.

What to try (the same in all three)
  - Collapse and expand rooms in the tree on the left.
  - Click a participant (not yourself) to open volume, "Mute for me"
    and moderation actions.
  - Change the volume; the new value shows next to the name.
  - Type a chat message and press Enter.
  - Toggle Mic and Sound in the bottom bar.
  - Make the window narrow and see what stays readable.
  Saved servers, Devices, Move, Disconnect and Leave are placeholders.

What we want to hear back
  Which version feels better and why, anything that looks broken or
  unreadable, your Windows version and screen scale (100%, 125%...).
