# FRZ-W01 — worklog

Written for someone who has never seen the conversation that produced this
story. Update it whenever you finish a step, hit a dead end, or get blocked,
and commit it with the code.

## State
Started 2026-11-04T09:10Z on the Windows laptop, same session that closed
WIN-BUILD-000. Deviation, PO-visible: work is in the main checkout on `main`,
no worktree/branch, because the plan files are still uncommitted (same
deviation as WIN-BUILD-000). Nothing committed; the PO commits.
2026-11-04T10:05Z: Windows side done and green; story is **self-reported**
pending the macOS regression run (PO, needs the Mac) and a verifier.
WIN-CAP-001 started right after in the same checkout and replaced the stub
body of `system_audio_windows.rs`; the stub-state evidence below was taken
before that.

## Done
- `handy/src-tauri/build.rs`: `emit_system_audio_cfg()` at the top of
  `main()`. Prints `cargo:rustc-check-cfg=cfg(system_audio)` always and
  `cargo:rustc-cfg=system_audio` when `CARGO_CFG_TARGET_OS`/`_ARCH` is
  `macos`/`aarch64` or `windows`/`x86_64`. Reads cargo's target env vars, so
  it follows `--target` cross-checks.
- `audio_toolkit/audio/mod.rs`: module gates are now `cfg(all(system_audio,
  target_os = "macos"))` for `output_sensor.rs` / `system_audio.rs` and
  `cfg(all(system_audio, target_os = "windows"))` with `#[path]` for
  `output_sensor_windows.rs` / `system_audio_windows.rs`; both pairs mount as
  `mod output_sensor` / `mod system_audio`, so `pub use
  system_audio::SystemAudioRecorder` and `external_output_active()` are
  gated on plain `cfg(system_audio)` with no OS name.
- `managers/session.rs`: the six gates (`use`, `ActiveRecorder::System`
  variant, `stop`/`close` arms, both `build_recorder` arms) are
  `cfg(system_audio)` / `cfg(not(system_audio))`; the unsupported message is
  now `System audio capture is not available on this platform`.
- `lib.rs`: nothing to change. Its only macOS-aarch64 gate (line 2) is the
  Apple Intelligence module, out of scope.
- New `audio/output_sensor_windows.rs`: `external_output_active() -> false`
  via an empty `list_process_output()`; `ProcessAudio` +
  `any_external_output` mirrored with the macOS file's 4 tests plus one
  pinning the inert stub. Mirrored, not moved: moving would edit the macOS
  file body, which this story forbids.
- New `audio/system_audio_windows.rs` (stub state): IF-W01 surface, `open`,
  `start`, `stop` return `Err("not implemented: WIN-CAP-001")`, `close` Ok,
  `Drop` calls `close`; tests `surface_compiles_and_is_send` and
  `open_errs_until_win_cap_001`.
- `handy/mcp/db.ts`: `defaultDbPath(platform = process.platform, env =
  process.env)` branches on `win32` to `%APPDATA%\com.uppify.plaudy\history.db`
  (falls back to `~/AppData/Roaming` when `APPDATA` is unset), macOS path
  otherwise, `PLAUDE_DB` still wins. Injected parameters instead of mocking
  `process.platform` (it is a read-only getter in Bun; `defineProperty` works
  but leaks into other tests). `db.test.ts` covers both platforms, the
  fallback and the override: `bun test` 16 pass / 0 fail (was 14).
- `docs/adlc/INTERFACES.md`: IF-W01..W04 flipped to `frozen` with the final
  method list.
- Evidence, stub state, 2026-11-04T09:40Z:
  - `grep -rn 'all(target_os = "macos", target_arch = "aarch64")'` over
    `audio_toolkit/audio/`, `managers/session.rs`, `lib.rs` returns exactly
    `output_sensor.rs:16`, `system_audio.rs:12` (their `#![cfg]` lines) and
    `lib.rs:2` (Apple Intelligence, unrelated).
  - `cargo test --lib` on Windows: **370 passed, 0 failed, 0 ignored**
    (363 on `main` + 5 sensor tests + 2 stub tests), no
    `unexpected cfg` warning, 24 s.
  - `cargo build` succeeds (binary rebuilt for the WIN-CAP-001 check).

## In flight
Nothing half-changed.

## Tried and rejected
- `cargo check --target aarch64-apple-darwin` from Windows: not attempted.
  `transcribe-cpp-sys` runs cmake with the Metal feature for that target and
  `objc2-*`/`sherpa-onnx` need the Apple SDK; a cross `check` cannot get
  past the build scripts. Equivalence proof for macOS is the `rg` list plus
  the quoted `build.rs` logic (criterion 3); the PO runs `cargo test --lib`
  and `scripts/verify-bundle.sh` on the Mac.
- A shared `output_sensor_common.rs` for `ProcessAudio`/`any_external_output`:
  rejected because the macOS file would have to import it (a body edit).
- Mocking `process.platform` in the MCP test: rejected in favour of injected
  parameters (see Done).

## Blocked on
macOS regression run (PO, on the Mac), exact commands:

```
cd handy/src-tauri
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
export CMAKE_POLICY_VERSION_MINIMUM=3.5
cargo test --lib 2>&1 | tail -3        # expect the same count as on main (371)
grep -rn 'cfg(system_audio)\|cfg(all(system_audio' src | sort
cd ../.. && scripts/verify-bundle.sh   # after `bun tauri build`
```

Expected `grep` list on both platforms (the predicate sites):
`audio/mod.rs` x5 (two `mod` pairs + `pub use` + two `external_output_active`),
`managers/session.rs` x6. `build.rs` logic:

```
let supported = matches!((os, arch), ("macos", "aarch64") | ("windows", "x86_64"));
if supported { println!("cargo:rustc-cfg=system_audio"); }
```

## Next action
PO: run the macOS block above and paste the three outputs here. Verifier:
`plan/reviews/FRZ-W01.md` against the spec. Manual Windows check of the
Meeting button is folded into WIN-CAP-001's end-to-end run (the stub was
replaced the same day, so the "not available" message is only reachable on
non-x86_64 Windows now; the code path is exercised by the
`not(system_audio)` arm's compile on such targets, not on this laptop).
