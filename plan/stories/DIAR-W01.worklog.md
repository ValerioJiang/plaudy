# DIAR-W01 — worklog

Written for someone who has never seen the conversation that produced this
story. Update it whenever you finish a step, hit a dead end, or get blocked,
and commit it with the code.

## State
Started 2026-11-04T10:45Z on branch `DIAR-W01` in the main checkout (no
worktree: a worktree means a second 5-minute full build; the branch is the
isolation). Done 2026-11-04T11:10Z, **self-reported**. Two criteria need
the PO (macOS count, in-app speaker chips).

## Done
- `managers/diarization.rs`:
  - `diarize()` and `diarize_sortformer()`: platform cfg removed.
    `transcribe_cpp` is a base dependency on every target, so the Sortformer
    path compiles everywhere.
  - `diarize_sherpa()`: keeps `cfg(all(target_os = "macos", target_arch =
    "aarch64"))` (sherpa-onnx is only a dependency there) and gets a doc
    line; a new stub under the negated cfg logs `the sherpa engine is not
    built on this platform (Sortformer only, 4-speaker ceiling)` once per
    call and returns no turns. The old non-macOS `diarize` stub is gone.
  - `engine()` / `sherpa_available()` untouched: on Windows the ONNX pair is
    installed too, so an explicit `HANDY_DIAR_ENGINE=sherpa` resolves to
    `Sherpa` and lands on the stub (logged). The default resolves to
    Sortformer whenever its checkpoint is present, which the bundle
    guarantees.
  - Live tests `diarizer_ab` and `rediarize_recording` ungated and given
    `ensure_backends()`: dynamic-backend builds (Windows, Linux) must load the
    ggml modules in the test process (the app does it at startup in
    `TranscriptionManager`); the helper calls
    `transcribe_cpp::init_backends(<dir>)` with `HANDY_BACKENDS_DIR` or the
    test exe's grandparent (`target/debug`). Static macOS builds: no-op.
- Evidence (2026-11-04, Windows, debug):
  - `grep -n 'target_os = "macos", target_arch = "aarch64"'
    src/managers/diarization.rs` -> `367` (`diarize_sherpa`), `445` (stub).
  - Two-voice test file: Windows TTS, `Microsoft David Desktop` and
    `Microsoft Zira Desktop` alternating over 5 turns, 900 ms breaks, 56.7 s,
    16 kHz mono (`scratchpad/two-voices.wav`, regenerable with the
    PowerShell snippet in this session's notes; any two-voice 16 kHz WAV
    works).
  - `HANDY_DIAR_MODELS=%APPDATA%/com.uppify.plaudy/models HANDY_AB_WAV=...
    HANDY_AB_TRUTH=2 HANDY_AB_ENGINE=sortformer cargo test --lib diarizer_ab
    -- --ignored --nocapture`:
    `sortformer: using vulkan backend: Vulkan0` (the RTX 3060 Ti),
    `[ab] sortformer: 2 speakers, 15 turns, 13.7s (4x real time) <- matches
    truth`, `#1 24.6s (56%) / #2 19.4s (44%)` (David had 3 of the 5 turns).
  - Same run before the backend helper: `sortformer: failed to initialize
    CPU backend` -> 0 speakers. That failure is test-harness only; the app
    initialises backends at startup.
  - `cargo test --lib`: **381 passed, 0 failed, 4 ignored** (2 loopback
    live, 2 diarization live). Same pass count as before the change.

## In flight
Nothing half-changed. Files touched: `diarization.rs`, this worklog, the
spec, `plan/BACKLOG.md`, `plan/ATTENTION.md`.

## Tried and rejected
- Making `sherpa_available()` return false off-macOS so `engine()` can
  never pick Sherpa: rejected, it breaks the existing `engine()` unit tests
  (which construct the manager from model files alone) and changes macOS
  behaviour for no gain; the stub plus the log line covers the one
  reachable case.
- A shared predicate for "transcribe-cpp engines" alongside
  `cfg(system_audio)`: not needed, Sortformer needs no cfg at all.
- Running the in-app finalize path end to end: blocked on the ASR model
  download (Settings UI); left to the PO.

## Blocked on
PO:
1. macOS: `cargo test --lib` count unchanged vs `main`; `cargo test --lib
   diarizer_ab -- --ignored` still runs there (static backends -> no-op).
2. In-app on Windows: with the Nemotron model installed, run a Meeting (or
   play `two-voices.wav` through the speakers during a mic session) and
   confirm the History row shows two speaker chips and the log line
   `diarization: sortformer found 2 speaker(s)`.

## Next action
PO runs the two checks above, then commits the branch and merges. The
BASELINE §6 "0 speakers expected" note is superseded by this story.
