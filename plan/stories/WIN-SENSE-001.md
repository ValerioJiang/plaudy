# WIN-SENSE-001 — Windows output sensor: audio sessions, own PID excluded, meter-gated

## Context
Plaudy's opt-in auto-capture starts a meeting session when another app
begins to play audio. The decision logic (`managers/auto_capture.rs`,
`Supervisor::tick`) is pure and cross-platform; the sensor
`external_output_active()` is macOS-only (`audio/output_sensor.rs`, per-process
CoreAudio attribution with own-PID exclusion — read its header comment for
the v1 post-mortem). After FRZ-W01, `audio/output_sensor_windows.rs` is a
stub returning `false`. This story implements it with WASAPI audio sessions
(ADR-003). The supervisor, its constants and its tests must not change.

## Files it may touch
- `handy/src-tauri/src/audio_toolkit/audio/output_sensor_windows.rs`
- `handy/src-tauri/Cargo.toml` — ONLY feature flags on the existing `windows`
  dependency (`Win32_Media_Audio`, `Win32_System_Com`); no new crates.
- `plan/stories/WIN-SENSE-001.worklog.md`

Anything not listed here is a spec question, not a judgement call. Do not
touch `auto_capture.rs`, `audio/mod.rs`, `session.rs`, the macOS sensor.

## Interfaces it must respect
- IF-W02: `external_output_active() -> bool`, false on any error, never
  true for our own PID or for our own playback (the `playback_active()` mask
  lives in `mod.rs` and stays there).
- Reuse `any_external_output(&[ProcessAudio], own_pid)` from the shared
  module FRZ-W01 created (or mirror it with the same 4 tests if it did not).

## Acceptance criteria
- [ ] `list_process_output()` on Windows: `IMMDeviceEnumerator` → default
      render endpoint → `IAudioSessionManager2` → `IAudioSessionEnumerator`;
      for each session `IAudioSessionControl2::GetProcessId()` and
      `GetState()`; a session counts as "running output" iff state is
      `AudioSessionStateActive` **and** `IAudioMeterInformation::GetPeakValue()`
      for that session is above `0.001` (constant, documented). PID 0 /
      system sounds session is included like any other process.
- [ ] COM is initialised per call on the calling thread if not already
      (`CoInitializeEx` with `COINIT_MULTITHREADED`, tolerate
      `RPC_E_CHANGED_MODE`), and every interface is released; a 10-minute
      loop at 4 Hz shows flat memory in Task Manager (number in the worklog).
- [ ] Any `HRESULT` failure → empty list → `false` (log at debug level once
      per distinct error, not per tick).
- [ ] Unit tests: the pure decision tests pass on Windows; plus 2 `#[ignore]`
      live-acceptance tests mirroring the macOS ones: (a) silent machine →
      `false` for 5 s; (b) `powershell -c "(New-Object Media.SoundPlayer
      'C:\Windows\Media\tada.wav').PlaySync()"` in a loop → `true` within
      1 s. Both run by hand, outputs pasted.
- [ ] Live E2E with the supervisor (opt-in setting on): Teams test call
      starts → session auto-starts within `START_AFTER` + 1 s; hang up →
      finalizes after `STOP_AFTER`; Plaudy playing back its own recording
      does not trigger. Timestamps from the log in the worklog.
- [ ] Known-limitation check: with Chrome open on a silent tab and VS Code
      open, the sensor stays `false` (the meter gate is what makes this
      pass; note it).

## Out of scope
- App allowlist/denylist, settings UI, changing supervisor constants.

## Size
M

## Routing
Run on: Opus 5.5 at high — COM session enumeration and a trigger whose false
positives silently create junk sessions.
