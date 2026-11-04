# BASELINE-WINDOWS — WIN-BUILD-000 report

Story: `plan/stories/WIN-BUILD-000.md`. Branch `main`, commit `23acec0`, no
product code changed (`git diff handy/` is empty; `Cargo.toml` shows `M` only
because the Tauri CLI re-saved it, 0 bytes of diff).
Written 2026-11-04. Build ran 2026-11-03; tests, launch, MCP and inventory
ran 2026-11-04. Criteria marked **needs PO** need a human at the microphone;
each has a recipe in §9.

## 1. Toolchain inventory

| Item | Value | Source |
|------|-------|--------|
| rustc | 1.99.1 (58cf8f9c3 2026-10-13) | `rustc -V` |
| cargo | 1.99.1 (5f94df478 2026-08-27) | `cargo -V` |
| bun | 1.4.4 | `bun -v` (winget `Oven-sh.Bun`; **not on PATH**, exe under `%LOCALAPPDATA%\Microsoft\WinGet\Packages\Oven-sh.Bun_Microsoft.Winget.Source_8wekyb3d8bbwe\bun-windows-x64\`) |
| cmake | 4.2.1 | `cmake --version` |
| glslc | shaderc v2026.4 (spirv-tools v2026.4) | `glslc --version` |
| VULKAN_SDK | `C:\VulkanSDK\1.4.363.0` | env, machine + user level (winget `KhronosGroup.VulkanSDK`) |
| VS Build Tools | **18 (VS 2026)**, MSVC 14.44.35207 and 14.50.35717, `C:\Program Files (x86)\Microsoft Visual Studio\18\BuildTools` | directory listing (the spec and `BUILD.md` say VS 2022; 2026 works) |
| GPU / driver | NVIDIA GeForce RTX 3060 Ti, driver 610.88 | `nvidia-smi --query-gpu=name,driver_version` |
| Vulkan devices | RTX 3060 Ti (discrete, API 1.4.341, driver 610.88) and AMD Radeon(TM) Graphics (integrated, API 1.4.315, driver 2.0.353) | `vulkaninfo --summary` |
| Node | v24.21.0 | `node -v` |
| Git | present | — |

Nothing was missing at build time. Vulkan SDK and Bun had been installed the
same day with `winget install KhronosGroup.VulkanSDK` and
`winget install Oven-sh.Bun`.

## 2. Build

Command (Git Bash, with the PATH exports from `CLAUDE.md`):

```
cd handy && bun install          # 342 packages, 32.6 s
cd handy && bun tauri build --debug --no-bundle
```

`--no-bundle` because `tauri.conf.json` sets
`bundle.windows.signCommand = trusted-signing-cli ...` (Azure Trusted Signing,
CI only); bundling would fail with `program not found` after a successful
compile. That is a command-line flag, not a code change.

Final lines of the log (2026-11-03, full log was in the implementer's
scratchpad as `build1.log`):

```
warning: `handy` (lib) generated 4 warnings (run `cargo fix --lib -p handy` to apply 1 suggestion)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 4m 37s
       Built application at: C:\Users\vjiang-reply\Documents\plaudy\handy\src-tauri\target\debug\handy.exe
exit=0
```

- Wall time: **4 min 37 s** for the Rust side (after `tsc && vite build`),
  7 compiler warnings, all dead-code (`gated_external`,
  `is_session_toggle_click`, ...), none Windows-specific.
- `transcribe-cpp-sys v0.2.0` compiled with `dynamic-backends` + `vulkan`;
  the Vulkan shader generator ran without error. The `BUILD.md`
  "path-limit" troubleshooting was not needed. No CPU-only fallback was
  attempted because none was needed.
- Output: `handy/src-tauri/target/debug/handy.exe` (82,206,720 bytes).

## 3. `cargo test --lib`

Command: `cd handy/src-tauri && cargo test --lib` (2026-11-04).

```
    Finished `test` profile [unoptimized + debuginfo] target(s) in 32.55s
     Running unittests src\lib.rs (target\debug\deps\handy_app_lib-f007393742c85ef1.exe)
running 363 tests
...
test result: ok. 363 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.31s
```

**363 passed / 0 failed / 0 ignored**, 35 s wall. macOS reports 371: the
8 missing ones are the macOS-aarch64-gated tests (diarizer A/B, rediarize,
output-sensor live acceptance, etc.), compiled out here.

## 4. Runtime check

Launched `handy.exe` from its build directory (2026-11-04, local time
UTC+1; the app's own log is in UTC):

| Time | Event |
|------|-------|
| 09:28:01.26 | `Start-Process handy.exe`, PID 17268 |
| 08:28:02 (UTC) | `Seeded 69 catalog model(s)`; `Initializing database at "C:\Users\vjiang-reply\AppData\Roaming\com.uppify.plaudy\history.db"`; `Database migrated to version 7` |
| 08:28:02 (UTC) | `transcribe_cpp: load_backend: loaded Vulkan backend from ...\target\debug\ggml-vulkan.dll` |
| 08:28:02 (UTC) | `load_backend: loaded CPU backend from ...\ggml-cpu-cascadelake.dll` |
| 08:28:02 (UTC) | `transcribe_init_backends: 3 compute device(s) registered ...: Vulkan0, Vulkan1, CPU` |
| 08:28:02 (UTC) | `tray apply: icon=resources/tray_idle_dark.png menu=rebuilt busy=false took=3.5686ms` |
| 08:28:02 (UTC) | `Auto-capture supervisor started (system-audio trigger, opt-in).` |
| 08:28:02 (UTC) | WARN `Failed to apply autostart setting (enabled=false): The system cannot find the file specified. (os error 2)` |
| 09:28:31.27 | still alive after 30 s; `Stop-Process` |

- App launches, stays alive, no panic, nothing on stderr.
- Tray: the log shows the tray icon applied (`tray_idle_dark.png`); a visual
  check of the system tray is **needs PO**.
- The only warning is the autostart plugin looking for a shortcut that does
  not exist yet (debug build, never installed). Harmless; worth a glance in
  WIN-BUILD-001.
- A previous launch on 2026-11-03 17:08 UTC (first ever) installed the three
  bundled diarization models into the user directory.
- 60 s mic-only session to a `done` row with text: **needs PO** (§9.1).
- Meeting button: **needs PO** (§9.2). Expected log line, from
  `handy/src-tauri/src/managers/session.rs:852`, verbatim:
  `System audio capture is only available on Apple Silicon macOS`.

App data directories actually used (Tauri `identifier` = `com.uppify.plaudy`):

| Purpose | Path |
|---------|------|
| `history.db`, `recordings/`, `models/`, `settings_store.json` | `C:\Users\vjiang-reply\AppData\Roaming\com.uppify.plaudy` (= `%APPDATA%\com.uppify.plaudy`, as expected) |
| `logs\plaudy.log`, WebView2 profile (`EBWebView`) | `C:\Users\vjiang-reply\AppData\Local\com.uppify.plaudy` (= `%LOCALAPPDATA%\com.uppify.plaudy`) |

## 5. Transcription facts

- Default model by `RANK_OVERRIDES` (`src/catalog/mod.rs:75`):
  `handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf` rank 1,
  `parakeet-unified-en-0.6b-gguf` rank 2. Unchanged on Windows.
- **No ASR model is downloaded yet** (`%APPDATA%\com.uppify.plaudy\models`
  holds only `diarization/`). The 2026-11-03 log shows
  `Model unloaded manually`, nothing loaded.
- Backend: transcribe-cpp registered **Vulkan0, Vulkan1, CPU** and set
  `accelerator preference: Auto (applied on next model load)`. Which of the
  two Vulkan devices Auto picks (RTX 3060 Ti vs the AMD iGPU) is only logged
  at model load, so it is part of the PO recipe (§9.3). If it picks the iGPU
  the real-time factor will be poor and the accelerator setting must be
  forced; that is a fact FRZ-W01 and WIN-BUILD-001 need.
- Real-time factor on a 2-minute Italian recording: **needs PO** (§9.3).

## 6. Diarization facts

- `scripts/fetch-diarization-model.sh` ran under Git Bash on 2026-11-03:
  `handy/src-tauri/resources/models/diarization/sortformer.gguf`
  (139,310,336 bytes, sha256 verified). The app copied it, plus
  `segmentation.onnx` and `embedding.onnx`, to the user models dir on first
  launch.
- **Diarization does not run on Windows at all today, by construction.**
  `src/managers/diarization.rs:444` is the non-macOS-aarch64 stub:
  `pub fn diarize(&self, _samples: &[f32]) -> Vec<SpeakerTurn> { Vec::new() }`.
  Both engines (`diarize_sortformer` at :316, `diarize_sherpa` at :367) are
  gated `cfg(all(target_os = "macos", target_arch = "aarch64"))`, even though
  the Sortformer path only needs `transcribe_cpp`, which **is** built and
  loaded here. A 2-speaker recording will therefore finalize with zero
  speaker turns (transcript intact, no speaker labels). Recording it is still
  useful as the "before" sample; **needs PO** (§9.4).
- `HANDY_DIAR_ENGINE=sherpa` cannot be tested: `sherpa-onnx` is a dependency
  only under the macOS-aarch64 target section of `Cargo.toml`, so there is
  no sherpa code and no `onnxruntime.dll` for it on Windows. Shipping sherpa
  on Windows means adding the dependency for the `windows x86_64` target (an
  ADR, and a DLL for the installer).
- This changes the shape of the open ATTENTION item "Sortformer only vs
  sherpa": on Windows the real choice is between **ungating Sortformer**
  (small, needs no new DLL) and nothing.

## 7. MCP check

- `cd handy/mcp && bun test`: **14 pass, 0 fail, 74 expect() calls, 2 files,
  253 ms** (bun 1.4.4).
- Piped JSON-RPC over stdio with
  `PLAUDE_DB=C:\Users\vjiang-reply\AppData\Roaming\com.uppify.plaudy\history.db`
  (`initialize`, `notifications/initialized`, `tools/call list_sessions
  {limit:5}`), run as `bun run server.ts`:

```
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"plaude-local","version":"0.1.0"}}}
{"jsonrpc":"2.0","id":2,"result":{"content":[{"type":"text","text":"[]"}]}}
```

  The server opens the real Windows DB (WAL mode, app closed) and answers;
  `[]` because no session has been recorded yet. Re-run after §9.1 to see
  the row (§9.5).
- `defaultDbPath()` in `handy/mcp/db.ts:9` is the macOS path
  (`~/Library/Application Support/com.uppify.plaudy/history.db`), so on
  Windows the server **only works with `PLAUDE_DB` set**. That is the
  IF-W04 / WIN-BUILD-001 fact: the Windows branch must return
  `%APPDATA%\com.uppify.plaudy\history.db`.

## 8. DLL inventory of `handy/src-tauri/target/debug`

Files next to `handy.exe` (all from the 2026-11-03 build):

| Group | File | Size |
|-------|------|------|
| ggml core | `ggml.dll` | 68,096 |
| ggml core | `ggml-base.dll` | 655,872 |
| ggml backend | `ggml-vulkan.dll` | 74,451,456 |
| ggml backend (CPU variants, one is picked at runtime) | `ggml-cpu-x64.dll`, `-sse42`, `-sandybridge`, `-haswell`, `-skylakex`, `-cannonlake`, `-cascadelake`, `-icelake`, `-alderlake` | 0.77–1.02 MB each |
| transcribe-cpp | `transcribe.dll` | 1,798,656 |
| onnxruntime | **none** (`onnxruntime.dll` absent) | — |
| other | `DirectML.dll` | 18,527,776 |

Import tables (`dumpbin /dependents`):

- `handy.exe` imports `transcribe.dll`, `directml.dll`, `d3d12.dll`, `dxgi.dll`,
  `MSVCP140.dll`, `MSVCP140_1.dll`, plus system DLLs. No `onnxruntime.dll`
  import: the ONNX Runtime used by `transcribe-rs` (`features=["onnx"]`) is
  linked **statically** into `handy.exe` in this local build, with the
  DirectML EP alongside as `DirectML.dll`. The `Cargo.toml` comment says CI
  links a baseline ONNX Runtime dynamically through `ORT_LIB_LOCATION`; that
  env var is not set on this laptop, so the local build differs from CI here.
  WIN-BUILD-001 must decide which one the installer ships (the prebuilt one
  carries the pre-Haswell AVX2 crash noted in `Cargo.toml`).
- `transcribe.dll` imports `ggml.dll`, `ggml-base.dll`, `VCRUNTIME140(_1).dll`,
  `MSVCP140.dll`. Backends are `LoadLibrary`'d from the exe directory at
  runtime (log: `scanning C:\...\target\debug`), so every `ggml-*.dll` must
  sit next to the exe in the bundle.
- `ggml-vulkan.dll` imports `vulkan-1.dll` (system Vulkan loader, from the
  GPU driver; not shipped by us) and `ggml-base.dll`.
- VC++ runtime (`MSVCP140`, `VCRUNTIME140`, `VCRUNTIME140_1`) is required by
  every DLL above: the installer needs the VC++ 2015-2022 redistributable or
  the DLLs bundled.

Bundled resources are copied to `target/debug/resources/` (default settings,
sounds, tray PNGs, `models/diarization/{sortformer.gguf,segmentation.onnx,embedding.onnx}`,
`models/silero_vad_v4.onnx`, `gigaam_vocab.txt`).

## 9. PO recipes (interactive criteria)

Run from `handy/src-tauri/target/debug` with the PATH exports; keep
`%LOCALAPPDATA%\com.uppify.plaudy\logs\plaudy.log` open in a tail.

1. **60 s mic session.** Start `handy.exe`. In Settings pick the Nemotron
   model and let it download. Sessions panel, source Mic, Start, speak 60 s,
   Stop. Expect a History row with status `done` and text. Note the time and
   the row id.
2. **Meeting button.** Press Meeting once. Expect the quoted log line
   `System audio capture is only available on Apple Silicon macOS` and a
   mic-only session to continue. Paste the exact log line and timestamp.
3. **RTF.** Record ~2 min of Italian. From the log take the model name, the
   `load_backend`/device line at model load (which Vulkan device Auto chose),
   and the transcription duration; RTF = audio seconds / transcribe seconds.
4. **2 speakers.** Record a YouTube clip through the mic. Expect `0` speaker
   turns (§6); report what History shows. `HANDY_DIAR_ENGINE=sherpa`: skip,
   not compiled on Windows.
5. **MCP.** Re-run the stdio call from §7 with the app closed; the row from
   step 1 must appear.

Paste the results under a "PO results" heading at the end of this file.

## 10. Facts for FRZ-W01

All 29 occurrences of `cfg(all(target_os = "macos", target_arch = "aarch64"))`
(positive or negated) under `handy/src-tauri/src`, grouped by what they gate
(`grep -rn 'target_os = "macos", target_arch = "aarch64"' src`, 2026-11-04).
There are also 54 plain `cfg(target_os = "macos")` sites from upstream Handy;
none touch system audio, the sensor or diarization.

**A. System audio capture and output sensor — become `cfg(system_audio)` (IF-W03), 13 sites:**

| File:line | Gates |
|-----------|-------|
| `audio_toolkit/audio/mod.rs:4` | `mod output_sensor;` |
| `audio_toolkit/audio/mod.rs:8` | `mod system_audio;` |
| `audio_toolkit/audio/mod.rs:19` | `pub use system_audio::SystemAudioRecorder;` |
| `audio_toolkit/audio/mod.rs:53` | `external_output_active()` real |
| `audio_toolkit/audio/mod.rs:57` | `external_output_active()` stub `false` (negated) |
| `audio_toolkit/audio/output_sensor.rs:16` | whole module `#![cfg]` |
| `audio_toolkit/audio/system_audio.rs:12` | whole module `#![cfg]` |
| `managers/session.rs:26` | `use ...SystemAudioRecorder` |
| `managers/session.rs:205` | `ActiveRecorder::System` variant |
| `managers/session.rs:215` | `stop()` arm |
| `managers/session.rs:227` | `close()` arm |
| `managers/session.rs:836` | `Source::SystemAudio` start arm |
| `managers/session.rs:848` | `Source::SystemAudio` error arm (negated; message at :852) |

**B. Diarization — 6 sites, not covered by IF-W03 as drafted:**

| File:line | Gates |
|-----------|-------|
| `managers/diarization.rs:298` | `diarize()` real |
| `managers/diarization.rs:316` | `diarize_sortformer()` (needs only `transcribe_cpp`) |
| `managers/diarization.rs:367` | `diarize_sherpa()` (needs `sherpa_onnx`, macOS-only dep) |
| `managers/diarization.rs:444` | `diarize()` stub returning empty (negated) |
| `managers/diarization.rs:1330` | `#[ignore]` test `diarizer_ab` |
| `managers/diarization.rs:1435` | `#[ignore]` test `rediarize_recording` |

Recommendation for the freeze: split this gate. `diarize`/`diarize_sortformer`
should follow a predicate that is true wherever `transcribe_cpp` is built
(macOS aarch64 and Windows x86_64), and only `diarize_sherpa` stays
macOS-only. Otherwise Windows ships without speaker labels.

**C. Apple Intelligence — stay as they are, 10 sites:**
`actions.rs:1,201,236`; `commands/mod.rs:121,125`; `lib.rs:2`;
`settings.rs:691`; `shortcut/mod.rs:21,1208,1213`.

**Vulkan: GO.** transcribe-cpp with `vulkan` compiles on this laptop with
VS 2026 + Vulkan SDK 1.4.363.0 in 4 min 37 s, `ggml-vulkan.dll` loads at
startup, and two Vulkan devices plus CPU register. Not yet shown: an actual
transcription on the RTX 3060 Ti (needs the model download, §9.3) and which
Vulkan device Auto selects. Nothing in the plan needs renegotiating.

**Other facts the freeze and Wave 1 depend on**

- `defaultDbPath()` in `handy/mcp/db.ts` is macOS-only (§7).
- Diarization is stubbed out on Windows (§6); sherpa is not a Windows dep.
- ONNX Runtime is linked statically in the local build; CI expects a
  dynamic one via `ORT_LIB_LOCATION` (§8).
- All ggml backends are loaded by scanning the exe directory; the NSIS bundle
  must place them beside the exe, plus the VC++ runtime (§8).
- Autostart plugin warns on a debug build (§4).
- The `bundle.windows.signCommand` makes any local `tauri build` without
  `--no-bundle` fail; WIN-BUILD-001 needs a local-build path around it.
