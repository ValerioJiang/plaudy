# Handoff — session 1 (Windows port): baseline done, freeze and loopback landed

## Where we are
2026-11-04. Three stories moved in one session on the Windows laptop, all
in the main checkout on `main` with nothing committed (plan files were still
untracked, so no worktrees; the PO commits):

- **WIN-BUILD-000** self-reported. Report `docs/adlc/BASELINE-WINDOWS.md`:
  build 4m37s, `cargo test --lib` 363 green, MCP 14 green, app launches on
  Vulkan (RTX 3060 Ti + AMD iGPU + CPU registered). PO recipes in §9 open.
  Key facts: diarization is compiled out on Windows (Sortformer included,
  `diarization.rs:444` stub); sherpa is not a Windows dependency; MCP
  default DB path was macOS-only; ORT statically linked locally.
- **FRZ-W01** self-reported. `cfg(system_audio)` from `build.rs`, module
  pairs mounted by `target_os` in `audio/mod.rs`, `session.rs` on the
  predicate, Windows stub sensor, MCP `defaultDbPath` platform branch with
  tests (16 green). IF-W01..W04 `frozen` in `docs/adlc/INTERFACES.md`.
  Windows `cargo test --lib` 370 green at stub state. **macOS regression run
  pending (PO, needs the Mac; commands in the worklog).**
- **WIN-CAP-001** self-reported. Real WASAPI loopback recorder in
  `audio/system_audio_windows.rs` (default render endpoint, shared mode,
  event-driven, mix-format conversion, clock-disciplined zero fill, device
  loss tolerated). `cargo test --lib` **381 green, 2 ignored live tests
  that pass on the laptop**. End-to-end Meeting session: system track
  31.02 s vs mic 30.90 s vs wall 31.01 s, silence preserved, mixed WAV
  written. **The other side of a Teams/Meet call is now recorded on
  Windows.** Pending PO: device-unplug check and the 10-minute Teams
  alignment run (worklog "Blocked on").

Toolchain unchanged from session 0 (Rust 1.99.1, VS 2026 Build Tools,
Bun 1.4.4 not on PATH, Vulkan SDK 1.4.363.0). Build hygiene: `bun tauri
build --debug --no-bundle` or plain `cargo build` in `handy/src-tauri`
(frontend `dist/` from 2026-11-03 is embedded); `dumpbin` must run from
PowerShell, not Git Bash.

## Wave in progress
Wave 1 is open. WIN-CAP-001, WIN-CAP-003, DIAR-W01 and WIN-SENSE-001 done;
WIN-BUILD-001 (NSIS, DLLs, MCP path, BUILD.md) remains spec'd and
untouched. Its shared files are frozen.

## Decided since last handoff
None by the PO. Implementer choices worth a glance: no silent render stream
(clock fill suffices); `AvSetMmThreadCharacteristicsW` skipped; GUIDs as
local constants instead of two more `windows` feature sets; MCP test uses
injected `platform`/`env` parameters instead of mocking `process.platform`.

## Interfaces frozen since last handoff
IF-W01, IF-W02, IF-W03, IF-W04 (2026-11-04). Diarization gates are NOT under
IF-W03; DIAR-W01 removed them from the Sortformer path instead.

## Open questions for the Product Owner
See `plan/ATTENTION.md`. New this session: ungate Sortformer diarization
on Windows (small change, no new DLL), otherwise Windows meetings have no
speaker labels; the earlier Sortformer-vs-sherpa item is moot as posed
(sherpa cannot be enabled without an ADR adding the dependency). Still open
from session 0: macOS regression run; consent/retention; commit the ADLC
files. Plus the PO recipes in `BASELINE-WINDOWS.md` §9 and the two WIN-CAP-001
recipes.

## Next action
1. PO: commit the ADLC scaffold and this session's code (suggested: one
   commit per story, messages starting with the story ID).
2. PO: download the Nemotron model in the app, record a real Teams call
   with the Meeting button, then run the WIN-CAP-001 alignment recipe and the
   BASELINE §9 recipes in one sitting.
3. PO on the Mac: FRZ-W01 regression block.
4. DIAR-W01 (diarization gate) and WIN-CAP-003 (every active render
   endpoint, earphones/Bluetooth, device rescan) are done and self-reported,
   both uncommitted in the main checkout on branch `DIAR-W01` (disjoint
   files: commit by path, two commits). Sortformer finds 2 speakers on a
   two-voice WAV on Windows via Vulkan; the E2E Meeting captured three
   endpoints including the WH-1000XM4 with mic and system tracks of
   identical length. Both committed and merged to main (f3ba71e).
5. WIN-SENSE-001 done and self-reported on branch `WIN-SENSE-001`
   (uncommitted at hand-off): real WASAPI audio-session sensor, every
   active render endpoint, own PID excluded, peak-meter gated (0.001).
   PowerShell tada.wav triggers in 322 ms; the soak found a ~2 KB/tick leak
   from per-call COM teardown, fixed with a thread-held COM init; 10-minute
   4 Hz soak then flat.
   Deviation from the spec text (all endpoints, not default) is a DECIDE
   in ATTENTION. PO still owes: silent-machine run, supervisor E2E on a
   Teams test call, Chrome-silent-tab check (recipes in the worklog). The
   supervisor E2E was deliberately not run from the session because Teams
   was open and Chrome was playing on the laptop.
6. DATA-001 (meeting audio retention) and DATA-002 (FTS5 trigram search
   index) done and self-reported in the same uncommitted tree, branch
   `WIN-SENSE-001`. Commit by path in this order: WIN-SENSE-001, DATA-001,
   DATA-002 (`history.rs` is shared by the two DATA stories; migration #8 is
   DATA-001's, #9 DATA-002's). Old retention deleted whole meeting rows after
   5; now sessions are never deleted, only their audio after 7 days once
   transcribed. The laptop DB is already migrated to version 9 by the debug
   launches that regenerated `bindings.ts`. PM: next is WIN-BUILD-001, then
   EVAL-W01.

## Known unknowns
Which Vulkan device `Auto` picks at model load (RTX vs iGPU); loopback
behaviour across a default-device switch on this laptop (recipe written,
not run); whether 5 % CPU during a session holds on battery; Sortformer on
Vulkan.
