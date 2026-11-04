# Interfaces (Windows port)

Nothing parallelises until the contracts here are frozen. Wave 0 is always
interface freezing. All four entries were frozen by FRZ-W01 on 2026-11-04.

Status legend: `draft` — no story may depend on it; `frozen` — a contract,
changing it is a PM decision raised as `STOP INTERFACE-CHANGE`.

## Entries

### IF-W01 — `SystemAudioRecorder` surface (Windows must match macOS)
- Status: frozen (FRZ-W01, 2026-11-04)
- Kind: module boundary
- Contract: `handy/src-tauri/src/audio_toolkit/audio/system_audio_windows.rs`
  exposes `pub struct SystemAudioRecorder` with exactly the macOS surface used
  by `session.rs::build_recorder`:
  `new() -> Result<Self, Box<dyn Error>>`, `with_level_callback(cb)`,
  `with_chunk_sink(mpsc::Sender<Vec<f32>>)`, `open()`, `start()`,
  `stop() -> Result<Vec<f32>>`, `close()`, `Drop`. Frames reach the sink as
  **16 kHz mono f32** through the shared `run_consumer` (same as the macOS
  IOProc path), `AudioChunk::EndOfStream` emitted exactly once on stop.
  Silence on the loopback must still produce frames (zeros) so wall-clock
  length is preserved. The struct must be `Send`.
- Final surface, copied from `system_audio_windows.rs` (identical to `system_audio.rs`):
  ```
  pub struct SystemAudioRecorder;                       // Send
  pub fn new() -> Result<Self, Box<dyn Error>>
  pub fn with_level_callback<F: Fn(Vec<f32>) + Send + Sync + 'static>(self, cb: F) -> Self
  pub fn with_chunk_sink(self, sink: mpsc::Sender<Vec<f32>>) -> Self
  pub fn open(&mut self) -> Result<(), Box<dyn Error>>      // idempotent while open
  pub fn start(&self) -> Result<(), Box<dyn Error>>
  pub fn stop(&self) -> Result<Vec<f32>, Box<dyn Error>>    // drained 16 kHz mono; empty when a chunk sink took the frames
  pub fn close(&mut self) -> Result<(), Box<dyn Error>>     // safe twice; Drop calls it
  ```
  Producer contract with `run_consumer`: raw frames at the device rate as
  `AudioChunk::Samples`; while the shared `stop_flag` is set, one
  `AudioChunk::EndOfStream` then silence (same as the cpal callback).
- Consumers: `session.rs` (`ActiveRecorder::System`), WIN-CAP-001, EVAL-W01.

### IF-W02 — `external_output_active()` on Windows
- Status: frozen (FRZ-W01, 2026-11-04)
- Kind: module boundary
- Contract: `audio/mod.rs::external_output_active() -> bool` keeps its
  signature and semantics: true iff a process other than ours is emitting
  audio right now; false on any error; masked by `playback_active()`.
  Windows implementation in `audio/output_sensor_windows.rs`; the pure
  decision `any_external_output(&[ProcessAudio], own_pid)` is mirrored
  (not shared) in `output_sensor_windows.rs` with the same four tests, so the
  macOS file body stays untouched. Until WIN-SENSE-001 the Windows
  `list_process_output()` returns an empty list (sensor inert).
- Consumers: `auto_capture.rs::run_supervisor`, WIN-SENSE-001.

### IF-W03 — The platform predicate
- Status: frozen (FRZ-W01, 2026-11-04)
- Kind: build contract
- Contract: one predicate, `cfg(system_audio)`, emitted by
  `handy/src-tauri/build.rs` as `cargo:rustc-cfg=system_audio` when the
  target is `macos && aarch64` or `windows && x86_64`. Every former
  `cfg(all(target_os = "macos", target_arch = "aarch64"))` around system
  audio or the sensor becomes `cfg(system_audio)`; the module file chosen
  underneath is selected by `target_os`. No story after FRZ-W01 may add a
  new OS cfg to `session.rs`, `lib.rs` or `audio/mod.rs`.
- As landed: `build.rs::emit_system_audio_cfg()` (also emits
  `rustc-check-cfg`); `audio/mod.rs` mounts `output_sensor.rs` /
  `system_audio.rs` under `cfg(all(system_audio, target_os = "macos"))` and
  `output_sensor_windows.rs` / `system_audio_windows.rs` under
  `cfg(all(system_audio, target_os = "windows"))` via `#[path]`, both as
  `mod output_sensor` / `mod system_audio`. Six `cfg(system_audio)` sites in
  `session.rs`, three in `audio/mod.rs` besides the mounts. Note for the
  PM: the diarization gates in `managers/diarization.rs` (Sortformer and
  sherpa) are NOT covered by this predicate and still read macOS-aarch64;
  see `BASELINE-WINDOWS.md` §10-B.
- Consumers: every Wave 1 story.

### IF-W04 — App data path shared by the app and the MCP server
- Status: frozen (FRZ-W01, 2026-11-04)
- Kind: path contract
- Contract: `history.db` lives in Tauri's `app_data_dir()` for identifier
  `com.uppify.plaudy`: `~/Library/Application Support/com.uppify.plaudy/` on
  macOS, `%APPDATA%\com.uppify.plaudy\` on Windows. `handy/mcp/db.ts::
  defaultDbPath()` branches on `process.platform` to the same two paths;
  `PLAUDE_DB` still overrides. A test asserts both branches.
- As landed: `defaultDbPath(platform = process.platform, env = process.env)`;
  on `win32` uses `env.APPDATA`, falling back to `~/AppData/Roaming`.
  Verified against the real Windows DB (`list_sessions` over stdio) in
  `BASELINE-WINDOWS.md` §7.
- Consumers: WIN-BUILD-001, `handy/mcp/*`.
