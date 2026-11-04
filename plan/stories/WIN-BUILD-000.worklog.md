# WIN-BUILD-000 — worklog

Written for someone who has never seen the conversation that produced this
story. Update it whenever you finish a step, hit a dead end, or get blocked,
and commit it with the code.

## State
Started 2026-11-03T16:50Z (approx). Working in the main checkout on branch
`main` (PM-authorised deviation: no worktree/branch because plan files are
uncommitted; nothing is committed by the implementer, the Product Owner commits).
2026-11-04T08:30Z: all non-interactive criteria done, report written at
`docs/adlc/BASELINE-WINDOWS.md`. Story is **self-reported** pending the PO
recipes (report §9) and a verifier. The first implementer session ended
after launching build 1 and never came back; a second session picked up on
2026-11-04 from the build log.

## Done
- Toolchain inventory (rustc/cargo 1.99.1, bun 1.4.4, cmake 4.2.1, glslc
  shaderc v2026.4, VULKAN_SDK=C:\VulkanSDK\1.4.363.0, driver 610.88, RTX 3060 Ti,
  plus an AMD Radeon iGPU as second Vulkan device).
  NOTE: VS Build Tools is **2026 (v18.1)**, MSVC 14.44 and 14.50, not 2022.
- `bun install` in handy/: 342 packages, 32.6 s.
- `scripts/fetch-diarization-model.sh` run under Git Bash: `sortformer.gguf`
  (139,310,336 bytes) present and sha256-verified.
- `rg`/grep of the macOS-aarch64 cfg: 29 occurrences in 10 files, classified
  in report §10 (13 system-audio/sensor, 6 diarization, 10 Apple Intelligence).
- Build 1 (2026-11-03): `bun tauri build --debug --no-bundle`, exit 0,
  Rust side 4m 37s, 7 dead-code warnings, `target/debug/handy.exe`.
- `cargo test --lib` (2026-11-04): 363 passed / 0 failed / 0 ignored, 35 s.
- `cd handy/mcp && bun test`: 14 pass / 0 fail.
- MCP stdio `list_sessions` with `PLAUDE_DB` pointing at the real Windows
  `history.db`: answers `[]` (DB exists, 0 sessions). `defaultDbPath()` is
  macOS-only, so `PLAUDE_DB` is mandatory on Windows today.
- 30 s launch of `handy.exe` (2026-11-04 09:28 local): alive, Vulkan backend
  loaded, 3 compute devices (Vulkan0, Vulkan1, CPU), tray applied, DB at
  `%APPDATA%\com.uppify.plaudy\history.db`, log at
  `%LOCALAPPDATA%\com.uppify.plaudy\logs\plaudy.log`. One WARN (autostart
  shortcut missing, debug build).
- DLL inventory + `dumpbin /dependents` of handy.exe, transcribe.dll,
  ggml-vulkan.dll, ggml.dll, DirectML.dll: in report §8. No onnxruntime.dll
  (ORT statically linked locally; CI uses ORT_LIB_LOCATION).
- Finding worth the PM's eyes: diarization is a no-op on Windows by
  construction (`diarization.rs:444` stub), Sortformer included, although
  transcribe-cpp is built and loaded here. sherpa-onnx is not a Windows
  dependency at all. Report §6 and §10-B.

## In flight
Nothing. No source file is changed. Files touched by this story: this
worklog and `docs/adlc/BASELINE-WINDOWS.md` only. `handy/src-tauri/Cargo.toml`
shows `M` in `git status` with an empty `git diff` (Tauri CLI re-saved it;
only the line-ending warning), not a content change.

## Tried and rejected
- `bun tauri build --debug` WITHOUT `--no-bundle`: not attempted on purpose.
  `tauri.conf.json` has `bundle.windows.signCommand = trusted-signing-cli ...`
  (Azure Trusted Signing, only in release CI) so the bundling step would fail
  with `program not found` after a successful compile (documented in
  `handy/BUILD.md`). Used `--debug --no-bundle`, which is a command-line flag,
  not a code change.
- `bun` is not on the shell PATH and the WinGet `Links` alias directory does
  not contain `bun.exe`; used the package dir
  `...\Packages\Oven-sh.Bun_Microsoft.Winget.Source_8wekyb3d8bbwe\bun-windows-x64`
  on PATH instead.
- `dumpbin /dependents` from Git Bash: MSYS turns `/nologo` into a path
  (`LNK1181: cannot open input file 'C:\Program Files\Git\nologo'`). Run it
  from PowerShell, or prefix `MSYS_NO_PATHCONV=1`.
- `HANDY_DIAR_ENGINE=sherpa` on Windows: not runnable, the sherpa code is not
  compiled for this target (Cargo.toml target section is macOS-aarch64 only).
- Downloading the Nemotron ASR model from the CLI to measure RTF without the
  PO: not done; the download goes through the Settings UI and the RTF needs
  a real Italian recording anyway. Left in the PO recipe (§9.3).

## Blocked on
Interactive criteria need the Product Owner (report §9): mic-only 60 s
session, Meeting button log line, 2-minute Italian mic recording RTF (and
which Vulkan device Auto picks at model load), 2-speaker clip (expected 0
speaker turns, see §6), MCP `list_sessions` on that session, visual tray check.

## Next action
PO runs report §9 and pastes results under a "PO results" heading at the end
of `docs/adlc/BASELINE-WINDOWS.md`. Then a verifier checks the story against
the spec. PM: read §10 before spec'ing FRZ-W01 (the diarization gate needs a
decision the current IF-W03 draft does not cover) and move the backlog row to
`self-reported`.
