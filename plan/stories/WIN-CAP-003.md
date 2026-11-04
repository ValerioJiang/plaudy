# WIN-CAP-003 — Capture every active render endpoint (earphones, Bluetooth hands-free) and follow device changes

## Context
WIN-CAP-001 opens one WASAPI loopback on the *default console* render endpoint
at session start. That misses two real meeting setups the PO uses: Bluetooth
earphones, whose "Hands-Free" endpoint is what Teams/Meet route a call to
(and which may not be the console default), and earphones plugged in after
the session started (the captured device gets invalidated and the session
records silence). PO request 2026-11-04: "even with earphones it works". The
recorder must capture whatever Windows is playing, wherever it plays it, for
the whole session.

## Files it may touch
- `handy/src-tauri/src/audio_toolkit/audio/system_audio_windows.rs`
- `handy/src-tauri/Cargo.toml` — ONLY feature flags on the existing `windows`
  dependency (endpoint enumeration, property store for friendly names)
- `plan/stories/WIN-CAP-003.worklog.md`, `plan/BACKLOG.md`

Anything not listed here is a spec question. Do not touch `session.rs`,
`recorder.rs`, `resampler.rs`, the macOS files, the frontend.

## Interfaces it must respect
- IF-W01 exactly: same `SystemAudioRecorder` surface and `run_consumer`
  contract; `EndOfStream` once per stop; struct `Send`.
- IF-W03: no new cfg in shared files.

## Acceptance criteria
- [x] `open()` enumerates `EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)`
      and starts one loopback capture per endpoint, each on its own COM
      thread, logging the friendly name and format of each. Evidence: log of
      the E2E session in the worklog (3 endpoints incl. the WH-1000XM4).
- [x] Per-endpoint streams are resampled to 16 kHz mono and summed on a
      master clock; a missing/late/dead endpoint is zero-padded, never stalls
      the mix. Unit tests: `mix_sums_endpoints_and_pads_the_short_one_with_silence`,
      `mix_with_no_endpoints_is_silence_of_the_requested_length`,
      `mix_clamps_and_leaves_the_remainder_queued`,
      `mix_trims_a_runaway_backlog_from_the_front`.
- [x] The endpoint list is rescanned every 2 s: new endpoints start
      capturing, vanished endpoints are retired, an endpoint whose stream
      died is retired and re-opened while still active. Evidence: code path
      + log lines `capturing "<name>" (new endpoint)` / `retiring "<name>"`;
      live plug/unplug run is the PO's (criterion below).
- [x] Wall-clock length preserved (R1) per endpoint and for the mix; the
      stop edge flushes the held-back tail so the track ends at the stop
      instant. Evidence: live test 3.000 s / 48 000 samples; E2E mic and
      system tracks both 370 560 samples over a 23.25 s session.
- [x] `cargo test --lib` green on Windows (385 passed, 4 ignored).
- [ ] PO: start a Meeting with speakers, plug in / connect the earphones
      mid-call, speak from the remote side, unplug again; the log shows the
      new endpoint captured and the remote voice is in the system track
      across the switch.
- [ ] PO: a Teams call over the Bluetooth hands-free profile with the
      Nemotron model installed; remote side transcribed and diarized.

## Out of scope
- Per-process loopback (WIN-CAP-002). Echo cancellation. Excluding our own
  playback from the capture (the macOS tap excludes by PID; here the
  `playback_active` mask only gates the sensor).

## Size
M

## Routing
Run on: the PO's session, Opus-class — threading and clock discipline across
N endpoints; the failure mode (one device silently missing) is invisible
until a real call.
