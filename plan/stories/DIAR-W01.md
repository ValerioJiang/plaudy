# DIAR-W01 — Sortformer diarization on Windows: ungate the engine, sherpa stays macOS-only

## Context
`managers/diarization.rs` gated `diarize`, `diarize_sortformer` and
`diarize_sherpa` behind `cfg(all(target_os = "macos", target_arch =
"aarch64"))`, with a non-macOS stub `diarize` returning no turns
(`BASELINE-WINDOWS.md` §6, §10-B). Sortformer only needs `transcribe_cpp`,
which is a dependency on every target and is loaded on Windows (Vulkan);
only sherpa-onnx is macOS-only in `Cargo.toml`. Without this story a
Windows meeting finalizes with a transcript but no speaker labels. PO
decision 2026-11-04: ungate Sortformer, keep sherpa macOS-only, no new
dependency.

## Files it may touch
- `handy/src-tauri/src/managers/diarization.rs` (cfg attributes on the three
  engine functions; a non-macOS `diarize_sherpa` stub; the two `#[ignore]`
  live tests ungated and given a backend-init helper)
- `plan/stories/DIAR-W01.worklog.md`
- `plan/BACKLOG.md`, `plan/ATTENTION.md` (status only)

Anything not listed here is a spec question, not a judgement call. No change
to `Cargo.toml`, `transcription.rs`, `session.rs`, the models or the UI.

## Interfaces it must respect
- IF-W03: no new OS cfg in `session.rs`, `lib.rs`, `audio/mod.rs` (this story
  touches none of them). The diarization gate is local to `diarization.rs`.
- `DiarizationManager` public surface unchanged: `new`, `is_available`,
  `diarize`, constants.

## Acceptance criteria
- [x] `diarize` and `diarize_sortformer` carry no platform cfg;
      `diarize_sherpa` keeps the macOS-aarch64 gate and a stub for other
      targets logs once and returns no turns. Evidence: `grep -n 'target_os
      = "macos", target_arch = "aarch64"' diarization.rs` lists exactly the
      sherpa pair (:367 and :445).
- [x] On Windows, `diarizer_ab` with `HANDY_AB_ENGINE=sortformer` on a
      two-speaker 16 kHz mono WAV reports 2 speakers. Evidence in the
      worklog: TTS two-voice file, `2 speakers, 15 turns, 13.7s (4x real
      time) <- matches truth`, split 56 % / 44 %, `using vulkan backend:
      Vulkan0`.
- [x] `cargo test --lib` on Windows green with no count regression
      (381 passed, 4 ignored: 2 loopback live + 2 diarization live).
- [ ] macOS: `cargo test --lib` count unchanged vs `main`; the two live
      tests still compile and run there (PO, needs the Mac).
- [ ] In-app: a Windows Meeting session with the Nemotron model installed
      finalizes with speaker chips in History (PO recipe in the worklog).

## Out of scope
- Shipping sherpa on Windows (would need an ADR adding the dependency and an
  onnxruntime DLL in the installer). Re-diarizing existing rows. Speaker
  naming / profiles (PROF-001).

## Size
S

## Routing
Run on: the PO's session, Opus-class — a three-attribute change whose risk is
entirely in what it unlocks, verified by a live run.
