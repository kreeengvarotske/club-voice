# Opus round-trip spike

Bounded stage 1 spike for [Issue #1](https://github.com/kreeengvarotske/club-voice/issues/1): encode generated PCM with Opus, decode it, and check that the result is complete, delayed by the expected amount and still carries the input content.

Scope: codec only. No audio devices, no network, no server, no wire format. Packet framing is still to be agreed in Issue #1.

## What it does

1. Generates deterministic 48 kHz mono 16-bit PCM: 2 s of three tones (300, 1000, 2500 Hz, each -14 dBFS peak) with 10 ms fades, then 0.5 s of digital silence.
2. Encodes it frame by frame with libopus (`OPUS_APPLICATION_VOIP`, 32 kbit/s target, VBR, default complexity, DTX and in-band FEC off).
3. Decodes every packet and measures:
   - packet count, decoded length and that each packet decodes to exactly one frame;
   - packet size (min/mean/max, silence) and payload bitrate;
   - codec delay: encoder lookahead versus a cross-correlation measurement on a separate 200 Hz to 4 kHz chirp (the tone mix repeats every 10 ms, so it cannot give an unambiguous delay);
   - level of each tone after decoding (Goertzel), sample SNR (coarse only, Opus is perceptual), and residual output during silence;
   - packet-loss concealment output for one missing packet;
   - that two fresh encoders produce byte-identical packets, plus a packet hash for comparing machines/builds.

## Commands

From the repository root.

Windows (PowerShell or cmd), with the Rust MSVC toolchain, Visual Studio C++ Build Tools and CMake on `PATH`:

```
cargo test --release --manifest-path spikes\opus-roundtrip\Cargo.toml
cargo run --release --manifest-path spikes\opus-roundtrip\Cargo.toml
cargo run --release --manifest-path spikes\opus-roundtrip\Cargo.toml -- --frame-ms 20 --bitrate 24000
```

If CMake is not on `PATH`, the copy bundled with Visual Studio Build Tools can be used by setting `CMAKE` to its `cmake.exe`. A short `CARGO_TARGET_DIR` (for example `C:\cv-target`) matches the verified Windows build and may help with Windows path-length limits in the libopus CMake build.

Linux/macOS: the same commands with `/` in the path (needs a C compiler and CMake).

libopus 1.6.1 is compiled from source bundled in `opusic-sys` 0.7.7 (via `opus` 0.4.0) and linked statically. No system libopus and no download at run time are required. `Cargo.lock` is committed so the dependency versions are fixed.

## Tests

`tests/roundtrip.rs`, for 10, 20, 40 and 60 ms frames:

- one packet per input frame, decoded length equals input length, every packet decodes to a full frame;
- packets between 1 and 1275 bytes; tone-segment payload within 0.6x to 1.4x of the target bitrate;
- measured delay within 8 samples of the encoder lookahead;
- every tone level within 1.5 dB of the input; sample SNR at least 4 dB;
- silence peak at most 16 (of 32767); PLC returns a full frame; encoding is deterministic.

Plus: 20 ms silence packets are at most 8 bytes, unsupported frame durations and partial frames are rejected, generated PCM is deterministic, 64 kbit/s costs clearly more than 16 kbit/s, and decoding continues with full frames after a lost packet.

## Results

Linux x86_64 (cloud container, Intel Xeon 2.3 GHz), Rust 1.97.0, release build, libopus 1.6.1, 32 kbit/s VOIP:

| Frame | Samples | Packet bytes min/mean/max | Silence packet max | Tone payload | Lookahead | Measured delay | Tone level change | SNR |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| 10 ms | 480 | 3 / 35.1 / 81 | 37 | 34.2 kbit/s | 312 (6.5 ms) | 310 | within 0.2 dB | 8.2 dB |
| 20 ms | 960 | 3 / 66.9 / 147 | 3 | 32.8 kbit/s | 312 (6.5 ms) | 310 | within 0.2 dB | 8.6 dB |
| 40 ms | 1920 | 5 / 127.8 / 159 | 5 | 31.6 kbit/s | 312 (6.5 ms) | 310 | within 0.3 dB | 8.6 dB |
| 60 ms | 2880 | 8 / 192.4 / 240 | 8 | 31.7 kbit/s | 312 (6.5 ms) | 310 | within 0.3 dB | 8.6 dB |

Encoding plus decoding 2.5 s of audio took about 11 ms in each case on that machine (single run, not a benchmark).

Windows (reported on PR #2 by the repository owner, commit f14d393): all 7 release-mode round-trip tests passed with bundled libopus 1.6.1 and the MSVC toolchain. The build used the `cmake.exe` bundled with Visual Studio Build Tools (passed through the `CMAKE` environment variable) and a short `CARGO_TARGET_DIR`. The `cargo run` report numbers for Windows are not recorded here yet.

## Notes for the voice path

- 20 ms is the proposed default frame: 960 samples at 48 kHz, about 67 bytes per packet at 32 kbit/s, 50 packets/s. Codec algorithmic delay is frame duration plus 6.5 ms lookahead.
- With DTX off, silence still sends a packet every frame (3 bytes at 20 ms). Whether to enable DTX, FEC or a lower bitrate is a later decision with real speech and loss tests.
- Generated tones are not speech. These results say nothing about perceived quality, noise suppression or real microphones; they only verify the codec integration and parameters.
