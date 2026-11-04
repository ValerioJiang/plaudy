# WIN-CAP-003 — worklog

Written for someone who has never seen the conversation that produced this
story. Update it whenever you finish a step, hit a dead end, or get blocked,
and commit it with the code.

## State
Started 2026-11-04T10:50Z in the main checkout (same session as DIAR-W01,
whose uncommitted change to `diarization.rs` sits beside this one; the two
stories touch disjoint files and should be committed separately by path).
Done 2026-11-04T11:50Z, **self-reported**. Two criteria need the PO with
earphones in hand.

## Done
- `audio/system_audio_windows.rs` restructured from one capture thread into
  a **coordinator + N endpoint threads**:
  - Coordinator (`plaudy-wasapi-mixer`): COM MTA, `IMMDeviceEnumerator`,
    `active_render_endpoints()` = `EnumAudioEndpoints(eRender,
    DEVICE_STATE_ACTIVE)` with friendly names from the property store
    (`PKEY_Device_FriendlyName`); spawns an `EndpointWorker` per endpoint;
    rescans every 2 s (`RESCAN_EVERY`), retiring vanished or dead workers and
    spawning new ones (a dead-but-active endpoint is re-opened that way);
    mixes with `mix_next()` on a master clock that runs `MIX_LATENCY` =
    100 ms behind wall time; stop edge flushes this tick's chunk plus the
    held-back tail (capped at 2x latency) before `EndOfStream`.
  - Endpoint thread (`plaudy-loopback:<name>`): own COM MTA, `GetDevice(id)`,
    the WIN-CAP-001 loopback open/drain/gap-fill loop unchanged, then
    `FrameResampler` to 16 kHz mono before sending to the coordinator. Any
    capture error raises `dead` and ends the thread.
  - `open()` now succeeds with zero endpoints (silence, logged) so a missing
    output device never costs the mic track; it fails only when COM or the
    enumerator cannot be created.
  - `run_consumer` is told 16 kHz, so its own resampler is a pass-through.
- `Cargo.toml`: `windows` features `Win32_UI_Shell_PropertiesSystem`,
  `Win32_Devices_FunctionDiscovery` (friendly names). No new crate.
- Tests: 4 new unit tests for `mix_next`; all WIN-CAP-001 tests kept.
  `cargo test --lib system_audio`: 17 passed, 2 ignored.
  `cargo test --lib`: **385 passed, 0 failed, 4 ignored**.
- Live test 1 (`live_three_seconds_have_three_seconds_of_frames`): 3.000 s,
  48 000 samples. (With the first cut, 200 ms latency and no stop flush, it
  was 2.820 s; the flush and the 100 ms latency fixed it.)
- Live test 2 (`live_played_sound_is_louder_than_silence`): 12.000 s; played
  window rms 0.153 vs 0.015 in the leading window (10x) while something else
  was already playing quietly.
- **E2E Meeting session** (2026-11-04 09:45 UTC, debug build, `--toggle-
  meeting` twice, ring tone 8 s, silence 6 s, ring tone 5 s): log
  `capturing "Digital Output (Realtek USB2.0 Audio)"`, `capturing "LG FHD
  (NVIDIA High Definition Audio)"`, `capturing "Headphones (WH-1000XM4)"`,
  each `48000 Hz, 2 ch, F32`; wall 23.25 s; `Session track Mic captured
  370560 samples`, `Session track SystemAudio captured 370560 samples`
  (23.16 s each, **identical length**); row saved, transcript pending (no
  ASR model on the laptop).

## In flight
Nothing half-changed. Files touched: `system_audio_windows.rs`,
`Cargo.toml` (2 feature lines), spec, this worklog, `plan/BACKLOG.md`.

## Tried and rejected
- Capturing the default *communications* endpoint instead of (or as well
  as) the console default: rejected, still misses a device that appears
  mid-call and depends on how each app picks its endpoint; enumerating all
  active endpoints covers every case with one rule.
- `IMMNotificationClient` callbacks for device changes instead of polling:
  rejected for now, a 2 s poll is simpler, has no COM callback lifetime to
  manage, and a 2 s gap at a device switch is below what a call needs.
- 200 ms mixer latency (first cut): the system track trailed the mic by
  ~190 ms and lost the last 200 ms at stop. Now 100 ms plus a stop flush.
  Expected residual offset: system track ~90 ms later than the mic for 48 kHz
  endpoints, bounded higher for an 8 kHz narrowband hands-free endpoint
  (128 ms resampler chunk). EVAL-W01 measures the real number.
- Re-basing the mixer to zero latency on `Cmd::Start`: cannot work, the
  mixer would always be ahead of what the endpoints have delivered.
- Excluding our own playback from the loopback: out of scope (see spec).

## Blocked on
PO, with earphones:
1. Meeting on speakers -> connect the WH-1000XM4 mid-call (or plug wired
   earphones) -> remote side speaks -> disconnect -> stop. Expect in the log
   `capturing "<earphones>" (new endpoint)` within 2 s of the switch and
   later `retiring "<earphones>" (endpoint gone)`; the remote voice must be
   audible across the switch in `recordings/<id>.wav`. Paste the lines and
   the two `captured N samples` values here.
2. Teams call over Bluetooth hands-free (the "Headset (... Hands-Free AG
   Audio)" endpoint, typically 8 or 16 kHz mono) with the Nemotron model:
   confirm the endpoint is listed as captured, the remote side is in the
   transcript, and the speaker chips appear (DIAR-W01).

## Next action
PO runs the two recipes. Verifier reviews against the spec. Then commit
(suggested message start: `WIN-CAP-003:`), separately from DIAR-W01.
