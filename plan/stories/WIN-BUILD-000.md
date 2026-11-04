# WIN-BUILD-000 — Baseline: build and run current `main` on Windows unchanged

## Context
Plaudy (this repo) is a Tauri 2 + Rust meeting recorder validated only on
Apple Silicon macOS. Handy upstream, which it forks, already builds on
Windows, and `handy/src-tauri/Cargo.toml` carries a Windows x86_64 section
(transcribe-cpp with `dynamic-backends` + `vulkan`, the `windows` crate).
Before porting anything we need the facts: what builds, what runs, what is
missing. This story changes no product code; it produces a report the freeze
story (FRZ-W01) and the risk register depend on. Reference laptop: Windows 11,
RTX 3060 Ti 8 GB, VS 2022 Build Tools present, Python 3.12, Node 24.

## Files it may touch
- `docs/adlc/BASELINE-WINDOWS.md` (new; the report)
- `plan/stories/WIN-BUILD-000.worklog.md`
- Nothing under `handy/` except untracked build outputs. If a one-line change
  is needed just to compile, stop and raise `STOP SPEC-AMBIGUOUS` with the
  exact error; do not patch.

## Interfaces it must respect
- None (read-only story). Report against the IF-W01..W04 drafts in
  `docs/adlc/INTERFACES.md` where relevant (e.g. which cfg gates exist).

## Acceptance criteria
- [ ] Toolchain inventory in the report: `rustc -V`, `cargo -V`, `bun -v`,
      `cmake --version`, `glslc --version`, `VULKAN_SDK`, VS Build Tools
      version, GPU driver version from `nvidia-smi`. Missing items listed with
      the exact `winget` command used to install them.
- [ ] `cd handy && bun install && bun tauri build --debug` (or `bun tauri
      dev` kept alive 60 s) completes; the report quotes the final lines and
      total wall time. If the Vulkan shader generator fails, the report
      includes the error and the result of retrying per `BUILD.md`
      "path-limit" troubleshooting; a CPU-only build
      (`transcribe-cpp` without `vulkan`) is attempted as a documented
      fallback **only by cargo feature flags on the command line**, not by
      editing Cargo.toml.
- [ ] `cd handy/src-tauri && cargo test --lib` result pasted: count passed /
      failed / ignored, and the names of any failing tests.
- [ ] Runtime check, recorded in the report with timestamps: the app
      launches; tray icon present; a 60 s **mic-only** session via the
      Sessions panel finalizes to a `done` history row with text; the
      **Meeting** button is pressed once and the log line for the skipped
      `SystemAudio` source is quoted verbatim (expected: "only available on
      Apple Silicon macOS").
- [ ] Transcription facts: which model was used (expected Nemotron Streaming
      3.5 by `RANK_OVERRIDES`), which backend loaded (Vulkan vs CPU, from the
      log), real-time factor on a 2-minute Italian recording made with the
      mic.
- [ ] Diarization facts: run `scripts/fetch-diarization-model.sh` (Git Bash)
      so `sortformer.gguf` exists; record a 2-speaker mic recording (PO +
      a colleague on speakerphone or a YouTube clip through the mic) and
      report speakers found; then repeat with `HANDY_DIAR_ENGINE=sherpa` and
      report whether the onnxruntime DLL is found.
- [ ] MCP check: `cd handy/mcp && bun test` result; then with
      `PLAUDE_DB=%APPDATA%\com.uppify.plaudy\history.db` a piped JSON-RPC
      `list_sessions` call returns the session above. Confirm the actual
      directory Tauri used for `history.db` (expected `%APPDATA%\com.uppify.plaudy`).
- [ ] DLL inventory of the debug build directory: list every `.dll` next to
      `handy.exe` / `plaudy.exe`, grouped as ggml backends, onnxruntime, other.
- [ ] The report ends with a "Facts for FRZ-W01" section: every
      `cfg(all(target_os = "macos", target_arch = "aarch64"))` occurrence in
      `handy/src-tauri/src` with file:line (use `rg`), and a go/no-go on Vulkan.

## Out of scope
- Any code change. Any Windows-on-ARM work. Installer/NSIS.

## Size
M (mostly waiting on builds)

## Routing
Run on: Sonnet 5.5 at high — mechanical build-and-report; judgement is in the
numbers, which the PM reads.
