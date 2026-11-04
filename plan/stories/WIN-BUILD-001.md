# WIN-BUILD-001 — Windows packaging: NSIS bundle with every DLL, bundle check script, MCP path, docs

## Context
After FRZ-W01 the MCP server knows the Windows DB path and the app builds on
Windows; after WIN-CAP-001/WIN-SENSE-001 it records meetings. What the PO
actually runs is the installer. On macOS two launch failures once shipped
green because `cargo test` never links the bundle (see
`docs/HANDOFF-BUNDLE.md`), and `scripts/verify-bundle.sh` now catches them.
This story makes the Windows installer trustworthy the same way: every ggml
backend DLL and the sherpa onnxruntime DLL ship and resolve, a PowerShell
bundle check resolves imports and launches the installed app for 30 s, and
`BUILD.md` documents the verified Windows path. Use the DLL inventory from
`docs/adlc/BASELINE-WINDOWS.md`.

## Files it may touch
- `handy/src-tauri/tauri.conf.json` — `bundle.windows`, `bundle.resources`,
  NSIS settings only
- `handy/src-tauri/nsis/installer.nsi` (if DLL copying needs it)
- `scripts/verify-bundle.ps1` (new) and a one-paragraph section in
  `scripts/verify-bundle.sh`'s header pointing to it
- `handy/BUILD.md` (Windows section: verified steps, Vulkan SDK, path-limit
  note, `fetch-diarization-model.sh` under Git Bash, `verify-bundle.ps1`)
- `handy/.github/workflows/build.yml` — ONLY if a Windows job exists and needs
  the new script; do not add CI runners (spec question)
- `handy/mcp/README.md` (Claude Code / Claude Desktop registration on
  Windows: `bun` path, `.mcp.json` works unchanged)
- `plan/stories/WIN-BUILD-001.worklog.md`

Anything not listed here is a spec question, not a judgement call. No Rust
or TypeScript source changes (the MCP path landed in FRZ-W01).

## Interfaces it must respect
- IF-W04 (path contract; verify, do not change).
- `bundle.identifier` stays `com.uppify.plaudy`.

## Acceptance criteria
- [ ] `bun tauri build` produces an NSIS `.exe` installer; installing it on
      the laptop into a fresh `%LOCALAPPDATA%\Programs\...` and launching
      works with no DLL error dialog; the installed directory listing is in
      the worklog.
- [ ] `scripts/verify-bundle.ps1 <install dir>`: (a) enumerates DLL imports
      of the main exe and of every shipped DLL (via `dumpbin /dependents`
      from VS Build Tools, or a documented fallback) and fails if any import
      is not resolvable from the install dir or `C:\Windows\System32`;
      (b) launches the exe, waits 30 s, fails if the process exited; (c)
      `PLAUDY_SKIP_SMOKE=1` limits it to (a). Exit codes documented. Run
      output pasted.
- [ ] A deliberate break (rename `ggml-vulkan.dll` in the install dir)
      makes the script fail with the missing name printed — evidence of the
      negative test.
- [ ] Sherpa fallback: with `HANDY_DIAR_ENGINE=sherpa` the installed app
      finds its onnxruntime DLL (shipped under the sherpa-specific name or
      subdirectory decided in the baseline) and diarizes a 2-speaker sample;
      or the worklog states sherpa is not shipped on Windows and why (PM
      decision recorded via `DECIDE OPTIONS`).
- [ ] `.mcp.json` from the repo root works from Claude Code on Windows with
      no edits (bun on PATH): `list_sessions` returns the installed app's
      sessions. Transcript of the call pasted.
- [ ] `BUILD.md` Windows section followed by the verifier on a clean clone
      produces a build; every command in it was actually run.

## Out of scope
- Code signing / SmartScreen reputation (document as roadmap). Auto-update.
  MSI target. ARM64.

## Size
M

## Routing
Run on: Sonnet 5.5 at high — configuration and scripting against documented
tools; two-key verification because an installer failure is silent to tests.
