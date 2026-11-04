# Decisions (ADLC / Windows port)

Append-only ADR log for the PO/PM-managed work on this repo. Newest at the
bottom. Never edit or delete an entry — supersede it with a new one. Product
decisions inherited from the macOS work live in `docs/DECISIONS.md` (the
riffado Keep/Spec/Drop ledger) and the `docs/HANDOFF-*.md` files; they are
not repeated here.

Format:

```
## ADR-000 — Title
- Date: YYYY-MM-DD
- Decision: what we are doing.
- Alternatives: what else was considered.
- Why: what made the difference.
- Supersedes: ADR-xxx (omit if none)
```

Model escalations and downgrades are logged here too, with their trigger.

---

## ADR-001 — Port Plaudy to Windows instead of building a new Python tool
- Date: 2026-11-03
- Decision: the Windows meeting-notes tool is Plaudy itself, built for
  Windows. The 20-story Python plan drafted the same day in
  `~/Documents/meeting-notes` is retired; its plan files stay there as an
  archive with a pointer here.
- Alternatives: (a) the Python plan (PyAudioWPatch + faster-whisper +
  pyannote + FastAPI + Claude API); (b) Meetily, which already runs on
  Windows but keeps diarization behind a PRO tier.
- Why: everything except two files is already cross-platform in this fork
  (Cargo has a Windows x86_64 section with the Vulkan ASR backend inherited
  from Handy; session manager, mixing, finalize, SQLite, MCP, tray, UI have
  no macOS cfg). The PO asked for "no APIs, just listen through system audio
  and mic" and for free or Claude-based summaries: Plaudy's MCP bridge
  already is that answer. Three stories replace twenty.

## ADR-002 — System audio on Windows: WASAPI loopback first, process loopback later
- Date: 2026-11-03
- Decision: `SystemAudioRecorder` on Windows captures the default render
  endpoint in loopback mode (`IAudioClient::Initialize` with
  `AUDCLNT_STREAMFLAGS_LOOPBACK`, event-driven, via the `windows` crate that
  is already a dependency). Per-process loopback
  (`ActivateAudioInterfaceAsync` + `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`,
  Windows 10 2004+), the exact analogue of the macOS Process Tap, is a
  follow-up story (WIN-CAP-002) once the device path is validated.
- Alternatives: `cpal` (no loopback support); the `wasapi` crate (works, but
  adds a dependency where `windows` already covers the API); a virtual audio
  cable (what Plaudy's README calls "local but clunky").
- Why: device loopback is the smallest change that reaches W2 and needs no
  permission prompt on Windows. Process loopback excludes notification
  sounds and music players, which is why it stays on the roadmap.
  Known trap, handled in WIN-CAP-001: a WASAPI loopback stream delivers no
  frames while nothing is rendering, so the recorder must fill wall-clock
  gaps with silence (or keep a silent render stream open) or the system
  track drifts against the mic track.

## ADR-003 — Output sensor on Windows: audio sessions, own PID excluded
- Date: 2026-11-03
- Decision: `external_output_active()` on Windows enumerates audio sessions
  on the default render endpoint (`IAudioSessionManager2` →
  `IAudioSessionEnumerator` → `IAudioSessionControl2::GetProcessId` /
  `GetState`), excludes our own PID, and treats a session as "emitting" only
  when `GetState == AudioSessionStateActive` and its
  `IAudioMeterInformation` peak is above a small threshold. Same shape as the
  macOS per-process sensor: all `unsafe` in one file, decision pure.
- Alternatives: device-level `IAudioMeterInformation` on the endpoint (cannot
  tell who is playing — the exact v1 mistake the macOS sensor already paid
  for); polling foreground window titles (reads the meeting client, which
  PRODUCT.md forbids).
- Why: per-process attribution is what made auto-capture trustworthy on
  macOS. The meter check closes the known limitation that apps holding an
  always-open output stream (Electron editors) read as "playing".

## ADR-004 — One cfg predicate for "has system audio", landed before Wave 1
- Date: 2026-11-03
- Decision: FRZ-W01 replaces every `cfg(all(target_os = "macos",
  target_arch = "aarch64"))` gate around system audio and the output sensor
  in `audio/mod.rs`, `session.rs` and `lib.rs` with a single alias
  (`cfg(system_audio)` set from `build.rs`, or an equivalent single
  predicate), and adds Windows modules that compile as stubs (`Err` /
  `false`). After FRZ-W01 the capture and sensor stories touch only their
  own new files.
- Alternatives: each story edits the shared files itself.
- Why: the PM parallelisation rule — stories run in parallel only on
  disjoint file sets. Centralising the predicate also keeps the macOS path
  byte-for-byte equivalent (W7).

## ADR-005 — Summaries stay on the MCP bridge; no in-app LLM, no API key
- Date: 2026-11-03
- Decision: unchanged from the macOS decision (HANDOFF-AUTOCAPTURE §12):
  Claude Code, running under the user's own subscription, summarizes on
  demand through `handy/mcp`. The Windows work only fixes the DB path
  (`%APPDATA%\com.uppify.plaudy\history.db`) and verifies Bun on Windows.
- Alternatives: the Claude API provider the Python plan had (≈ €0.10 per
  meeting on Opus 5.5); Ollama in-app.
- Why: the PO asked for free-or-Claude without APIs. The bridge is both.
