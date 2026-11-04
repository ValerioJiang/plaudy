# FRZ-W01 — Freeze IF-W01..W04: one `cfg(system_audio)` predicate, Windows stubs, MCP path contract

## Context
System-audio capture (`audio/system_audio.rs`) and the per-process output
sensor (`audio/output_sensor.rs`) are gated with
`cfg(all(target_os = "macos", target_arch = "aarch64"))` in
`audio/mod.rs`, `managers/session.rs` and `lib.rs`; on other targets
`SystemAudio` is refused with an error and the sensor returns `false`. The
Windows port needs those gates to admit `windows && x86_64` without touching
the shared files again in later stories (ADR-004). This story introduces the
predicate, adds Windows modules that compile as stubs with the exact IF-W01 /
IF-W02 surfaces, and fixes the MCP DB path contract (IF-W04). macOS behaviour
must be byte-identical. Read `docs/adlc/INTERFACES.md` and
`docs/adlc/BASELINE-WINDOWS.md` first.

## Files it may touch
- `handy/src-tauri/build.rs` (emit `cargo:rustc-cfg=system_audio` and
  `cargo:rustc-check-cfg=cfg(system_audio)` for `macos+aarch64` and
  `windows+x86_64`)
- `handy/src-tauri/src/audio_toolkit/audio/mod.rs` (gates → `cfg(system_audio)`;
  `#[cfg_attr]`/`#[cfg(target_os)]` selection of the backend module;
  `pub use` of `SystemAudioRecorder` from whichever module is compiled)
- `handy/src-tauri/src/audio_toolkit/audio/system_audio_windows.rs` (new stub:
  IF-W01 surface, every method returns `Err("not implemented: WIN-CAP-001")`
  except `new`/`with_*`; unit test that the surface compiles and `open()` errs)
- `handy/src-tauri/src/audio_toolkit/audio/output_sensor_windows.rs` (new stub:
  `external_output_active() -> false`; move `ProcessAudio` +
  `any_external_output` into a shared `output_sensor_common.rs` with their
  4 tests if the macOS file allows it without behaviour change, else mirror)
- `handy/src-tauri/src/managers/session.rs` (only the cfg attributes on
  `ActiveRecorder::System`, the `use`, `build_recorder` arms; the error
  message for unsupported targets becomes "System audio capture is not
  available on this platform")
- `handy/src-tauri/src/lib.rs` (only cfg attributes around the sensor /
  system-audio wiring, if any)
- `handy/mcp/db.ts` (`defaultDbPath()` branches on `process.platform ===
  "win32"` → `%APPDATA%\com.uppify.plaudy\history.db`), `handy/mcp/db.test.ts`
- `docs/adlc/INTERFACES.md` (flip IF-W01..W04 to `frozen`, record the final
  shapes)
- `plan/stories/FRZ-W01.worklog.md`

Anything not listed here is a spec question, not a judgement call. Do not
touch `system_audio.rs`, `output_sensor.rs` bodies, `auto_capture.rs`,
`Cargo.toml`, the frontend, or `tauri.conf.json`.

## Interfaces it must respect
- IF-W01, IF-W02, IF-W03, IF-W04 (this story freezes them; their text in
  `docs/adlc/INTERFACES.md` is the contract and may only be clarified, not
  changed, without `STOP INTERFACE-CHANGE`).
- Existing `Source` serde shape (`"Mic" | "SystemAudio"`) unchanged (tests in
  `session.rs` pin it).

## Acceptance criteria
- [ ] `rg 'all\(target_os = "macos", target_arch = "aarch64"\)' handy/src-tauri/src`
      returns only the two macOS backend files themselves (their `#![cfg]`
      line) and nothing in `mod.rs`, `session.rs`, `lib.rs`. Evidence: the
      command output.
- [ ] On Windows: `cargo build` succeeds; `cargo test --lib` passes with the
      new stub tests; pressing Meeting logs the new "not available on this
      platform" message and the mic track still records (manual check pasted).
- [ ] On macOS (the PO runs it; implementer prepares the exact commands):
      `cargo test --lib` count unchanged vs `main`, `scripts/verify-bundle.sh`
      passes, and `cargo expand`-free proof of equivalence: `rg -n
      "cfg\(system_audio\)"` list plus the `build.rs` logic quoted, so a
      reviewer can see macOS still compiles the same modules.
- [ ] `cargo check --target x86_64-pc-windows-msvc` and
      `cargo check --target aarch64-apple-darwin` both succeed from Windows
      (the latter may need `rustup target add`; if cross-check is impossible
      because of native deps, say so and show `cargo check` on both real
      machines instead).
- [ ] `cd handy/mcp && bun test` passes with a new test covering both
      platform branches of `defaultDbPath()` (mock `process.platform`).
- [ ] `docs/adlc/INTERFACES.md`: IF-W01..W04 `frozen`, with the final method
      list copied from the stub file.

## Out of scope
- Any real WASAPI or audio-session code (WIN-CAP-001, WIN-SENSE-001).
- Bundling, NSIS, DLLs (WIN-BUILD-001).

## Size
M

## Routing
Run on: Opus 5.5 at xhigh — frozen contract across two platforms, touches a
shipping macOS path; hard to reverse once Wave 1 fans out. Two-key verify.
