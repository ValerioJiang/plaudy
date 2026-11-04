# WIN-CAP-001 — WASAPI loopback `SystemAudioRecorder` for Windows

## Context
Plaudy records a meeting as two tracks: the microphone and the system audio
(the remote side of a Teams or Meet call). On macOS the system track comes
from a CoreAudio Process Tap in `audio/system_audio.rs`; after FRZ-W01 the
Windows build compiles `audio/system_audio_windows.rs` as a stub with the
same surface (IF-W01). This story fills that stub with a real WASAPI loopback
capture of the default render endpoint (ADR-002) using the `windows` crate
already in `Cargo.toml`. Downstream (PCM writer, mix, finalize, diarization)
is shared and must not change. Reference laptop: Windows 11, headset and
laptop speakers both in use by the PO.

## Files it may touch
- `handy/src-tauri/src/audio_toolkit/audio/system_audio_windows.rs`
- `handy/src-tauri/Cargo.toml` — ONLY to add feature flags to the existing
  `windows` dependency (`Win32_Media_Audio`, `Win32_System_Com`,
  `Win32_Media_KernelStreaming` or whatever the API needs); no new crates
  (a new crate is a spec question).
- `handy/src-tauri/src/audio_toolkit/audio/mod.rs` — ONLY if a `pub use`
  for a new helper is needed; no cfg changes.
- `plan/stories/WIN-CAP-001.worklog.md`

Anything not listed here is a spec question, not a judgement call. Do not
touch `session.rs`, `recorder.rs`, `lib.rs`, the macOS files, the frontend.

## Interfaces it must respect
- IF-W01 exactly: same methods and semantics as the macOS
  `SystemAudioRecorder`; frames delivered to the chunk sink as 16 kHz mono
  f32 through the shared `run_consumer` path used by the macOS IOProc (study
  `system_audio.rs` lines on `run_consumer`/`AudioChunk` and `recorder.rs`);
  `AudioChunk::EndOfStream` exactly once on `stop()`; the struct is `Send`.
- IF-W03: no new cfg in shared files.

## Acceptance criteria
- [ ] `open()` initialises `IAudioClient` on the default render endpoint
      (`IMMDeviceEnumerator::GetDefaultAudioEndpoint(eRender, eConsole)`) in
      shared mode with `AUDCLNT_STREAMFLAGS_LOOPBACK |
      AUDCLNT_STREAMFLAGS_EVENTCALLBACK`, using the endpoint's mix format;
      handles both `WAVE_FORMAT_IEEE_FLOAT` and 16/24/32-bit PCM
      (`WAVE_FORMAT_EXTENSIBLE` subformats) by converting to f32 and
      downmixing N channels to mono; unit tests cover the conversion and
      downmix functions with synthetic buffers (pure functions, no COM).
- [ ] `start()` spawns a capture thread (`CoInitializeEx` MTA on that thread,
      `AvSetMmThreadCharacteristicsW("Pro Audio")` optional) that waits on the
      event, drains `IAudioCaptureClient::GetBuffer` in a loop, honours
      `AUDCLNT_BUFFERFLAGS_SILENT` by emitting zeros, and pushes samples into
      `run_consumer`.
- [ ] **Silence keeps time (R1):** when no app renders audio, WASAPI stops
      delivering buffers. The thread tracks a monotonic clock and inserts
      zero frames so that total frames emitted == elapsed wall time × device
      rate within 0.5%. Test: a 60 s capture with 30 s of silence in the
      middle yields a system WAV of 60 s ± 0.3 s (manual recipe in the worklog
      with the measured number); plus a unit test of the gap-fill function
      with fake timestamps.
- [ ] Default-device change mid-session (headset unplugged) does not panic:
      the thread logs, emits silence until stop, and `stop()` still returns
      `Ok`. Manual test recorded in the worklog.
- [ ] `stop()` signals the thread, joins it, emits `EndOfStream` once, and
      returns the captured samples like the macOS impl (or an empty Vec if the
      sink took them — match macOS semantics exactly, read its code).
      `close()` releases COM objects; `Drop` is safe after `close()`.
- [ ] Alignment (W2): a 10-minute Meeting session on the laptop with a Teams
      test call where a remote participant speaks at known times; the system
      WAV and the mic WAV have equal length ± 200 ms and the merged timeline
      in History interleaves correctly. Numbers and screenshot path in the
      worklog.
- [ ] `cargo test --lib` green on Windows; `cargo check` for the macOS target
      unaffected (file is `#![cfg(all(target_os = "windows", target_arch = "x86_64"))]`).
- [ ] Worklog "Tried and rejected" records whether a silent render stream was
      needed to keep the loopback flowing, and the measured CPU% of the
      capture thread.

## Out of scope
- Per-process loopback (WIN-CAP-002). Device picker UI. Echo cancellation.

## Size
L

## Routing
Run on: Opus 5.5 at high — COM lifetime rules, realtime thread, clock
alignment; the failure mode (drift, silent truncation) is silent.
