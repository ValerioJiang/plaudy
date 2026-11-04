# Backlog — Plaudy for Windows

One row per story. Code lives in `handy/`; specs in `plan/stories/`.

| ID | Title | Status | Model | Deps | Branch |
|----|-------|--------|-------|------|--------|
| WIN-BUILD-000 | Baseline: build and run current `main` on the Windows laptop unchanged; record what works (mic session, Nemotron on Vulkan, Sortformer, MCP with `PLAUDE_DB`) | self-reported (2026-11-04; report `docs/adlc/BASELINE-WINDOWS.md`, PO recipes §9 open) | Sonnet 5.5 high | — | main |
| FRZ-W01 | Freeze IF-W01..W04: `cfg(system_audio)` predicate from build.rs, Windows stub modules for recorder and sensor, MCP path contract; macOS behaviour byte-identical | self-reported (2026-11-04; Windows green, macOS regression run pending) | Opus 5.5 xhigh | WIN-BUILD-000 | FRZ-W01 |
| WIN-CAP-001 | WASAPI loopback `SystemAudioRecorder` (default render endpoint, event-driven, silence-filled, 16 kHz mono f32 into the shared consumer) | self-reported (2026-11-04; live E2E on the laptop, Teams alignment run pending) | Opus 5.5 high | FRZ-W01 | WIN-CAP-001 |
| WIN-SENSE-001 | Windows output sensor: audio-session enumeration, own-PID excluded, meter-gated; supervisor unchanged | spec'd | Opus 5.5 high | FRZ-W01 | WIN-SENSE-001 |
| WIN-BUILD-001 | Windows packaging: MCP `defaultDbPath` branch, NSIS bundle with every backend DLL, `scripts/verify-bundle.ps1`, BUILD.md Windows section verified | spec'd | Sonnet 5.5 high | FRZ-W01 | WIN-BUILD-001 |
| WIN-CAP-003 | Capture every active render endpoint (earphones, Bluetooth hands-free), rescan for device changes, mix on a master clock | self-reported (2026-11-04; E2E captured 3 endpoints incl. the WH-1000XM4; PO plug/unplug + Teams-over-BT runs pending) | PO session, Opus-class | WIN-CAP-001 | — |
| DIAR-W01 | Sortformer diarization on Windows: `diarize`/`diarize_sortformer` ungated, sherpa stub off-macOS, live tests load ggml backends | self-reported (2026-11-04; 2 speakers on a two-voice WAV via Vulkan; macOS count + in-app chips pending) | PO session, Opus-class | WIN-BUILD-000 | DIAR-W01 |
| EVAL-W01 | Real-meeting validation on Windows: 3 Teams + 2 Meet calls (IT, EN, mixed), alignment, speaker counts, bleed, auto-capture; numbers in `docs/adlc/EVAL.md` | proposed | Sonnet 5.5 high | WIN-CAP-001, WIN-CAP-003, WIN-SENSE-001, WIN-BUILD-001, DIAR-W01 | — |
| WIN-CAP-002 | Per-process loopback (`ActivateAudioInterfaceAsync` PROCESS_LOOPBACK) targeting the process the sensor saw; falls back to device loopback | proposed | Opus 5.5 high | EVAL-W01 | — |
| PROF-001 | Cross-meeting voice profiles: name a speaker once, recognise them next time (embeddings from the sherpa path or a dedicated embedding model) | proposed | Opus 5.5 high | EVAL-W01 | — |

Status runs: `proposed` -> `spec'd` -> `running` -> `self-reported` -> `verified` ->
`merged`, or `rejected` / `blocked`.

`self-reported` and `verified` are different states on purpose. An implementer
moves a story to `self-reported`; only a verifier moves it to `verified`.

## Waves

- **Wave 0a — baseline.** WIN-BUILD-000 alone. It needs the toolchain on the
  laptop (Rust, Bun, Vulkan SDK, VS Build Tools) and produces the facts the
  freeze depends on: does Vulkan build, does Sortformer run, which DLLs the
  dev build loads.
- **Wave 0b — freeze.** FRZ-W01 alone. Lands the predicate and the stubs so
  Wave 1 stories touch only their own new files. Two-key verification (touches
  the macOS build).
- **Wave 1 — the port (parallel, disjoint).** WIN-CAP-001
  (`audio/system_audio_windows.rs` + its tests), WIN-SENSE-001
  (`audio/output_sensor_windows.rs` + tests), WIN-BUILD-001 (`handy/mcp/db.ts`,
  `tauri.conf.json` bundle section, `scripts/verify-bundle.ps1`, `BUILD.md`,
  CI workflow). Shared files were frozen in Wave 0b.
- **Wave 2 — evidence.** EVAL-W01 with the PO in real meetings.
- **Wave 3 — upgrades.** WIN-CAP-002, PROF-001, in that order, only if
  EVAL-W01 says they matter.

## Dependency graph and critical path

```
WIN-BUILD-000 ─> FRZ-W01 ─┬─> WIN-CAP-001 ───┐
                          ├─> WIN-SENSE-001 ─┼─> EVAL-W01 ─┬─> WIN-CAP-002
                          └─> WIN-BUILD-001 ─┘             └─> PROF-001
```

Critical path: WIN-BUILD-000 -> FRZ-W01 -> WIN-CAP-001 -> EVAL-W01. The one
fact that, if false, breaks the plan: that the Vulkan transcribe-cpp backend
builds and runs on this laptop (WIN-BUILD-000). If it does not, ASR falls back
to CPU and W3 is renegotiated; nothing else changes.

## Two-key stories
FRZ-W01 (changes cfg gates on a shipping macOS path), WIN-BUILD-001 (installer
is what the user runs; a missing DLL is a silent failure).
