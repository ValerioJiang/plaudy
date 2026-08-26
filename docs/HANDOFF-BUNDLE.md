# The two bugs that only the packaged app could show

Session of 2026-08-26, after the realign to Handy v0.9.6. Companion to
[HANDOFF.md](HANDOFF.md) and [HANDOFF-DIARIZATION.md](HANDOFF-DIARIZATION.md).

The realign ended with 371 Rust tests green, 14 MCP tests green, `cargo check --lib` clean, the
frontend built and linted, 23/23 locales complete. Then the installed app failed to launch —
twice, for two unrelated reasons. Both are worth recording, because the thing they have in common
is the interesting part: **nothing in the test suite can reach them.**

## 1. `Library not loaded: @rpath/libonnxruntime.dylib`

`bundle.macOS.frameworks` in `tauri.conf.json` is a hand-maintained list of dylib paths to copy
into `Contents/Frameworks/`. It named `target/release/libonnxruntime.1.24.4.dylib`.

The realign bumped the `ort` dependency, and the dylib it emits is now the unversioned
`libonnxruntime.dylib`. The versioned file still sat in `target/release/` — a stale artifact from
15 June — so the bundle step found it, copied it, signed it, and produced a bundle in which:

- `Contents/MacOS/handy` needs `@rpath/libonnxruntime.dylib`
- `libsherpa-onnx-c-api.dylib` needs `@rpath/libonnxruntime.dylib`
- `Contents/Frameworks/` contains only `libonnxruntime.1.24.4.dylib`

`cargo test` links against `target/`, where **both** files exist, so every test passed. Only the
`.app`, with its narrower `@rpath`, is missing the file.

Fixed by naming the dylib the binary actually links.

## 2. `state() called before manage() for SecureInputState`

With the dylibs resolved, the app aborted with `SIGABRT` inside
`applicationDidFinishLaunching`. Note that the crash report is nearly useless here: it shows
`abort()` under an unsymbolicated `handy` frame and nothing else. The panic message exists only on
stderr, which neither the `.ips` file nor the app's own log plugin captures — running the bundled
binary from a terminal is what produced it.

The cause is an interaction between the fork and upstream, not a line lost in the merge:

- Upstream registers Enigo and the global shortcuts **from the frontend**, after onboarding. So
  `secure_input::init` — which is what `manage`s `SecureInputState` — always ran first.
- The fork added an eager backend-side init inside `setup` (so that a webview that never loads
  cannot silently kill paste and shortcuts). It landed *above* `secure_input::init`.
- `initialize_shortcuts` → `shortcut::init_shortcuts` → `secure_input::reconcile_fallback` →
  `app.state::<SecureInputState>()` → panic, ten lines before the `manage()`.

Fixed by moving `secure_input::init` above the eager init, which is also exactly upstream's
ordering. The comment at the call site now says the ordering is load-bearing and why, because the
two statements look independent.

## What now catches both: `scripts/verify-bundle.sh`

Run it after every `bun tauri build`. It takes the `.app` (defaulting to the one in
`target/release/bundle/macos/`) and does two things:

1. **Link closure.** Reads the binary's `LC_RPATH` load commands rather than assuming them, then
   checks that every `@rpath/...` dependency of the executable *and of each bundled dylib*
   resolves inside the bundle. The dylib-to-dylib edge matters: sherpa's c-api needs onnxruntime,
   and a check that only looked at the executable would have called bug 1 healthy.
2. **Smoke launch.** Starts the app for 30 s and asserts it is still alive, printing the panic
   from stderr if not. Linking is necessary, not sufficient — bug 2 linked perfectly.
   `PLAUDY_SKIP_SMOKE=1` limits the run to the link check.

Both bugs reproduce from a clean checkout as script failures, and neither does as a test failure.

## The general lesson for this fork

Two of the fork's most valuable habits — extending upstream in place, and moving initialization
from the frontend into the backend — are exactly what makes it vulnerable here. A realign can
silently change what a build *emits* (bug 1) or what invariants the surrounding code *assumed*
(bug 2), and the fork's own divergence is what turns the change into a crash. A green suite says
the code is right; only the bundle says the product starts.
