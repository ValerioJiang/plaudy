# Product — Plaudy for Windows

## Goal
Bring Plaudy, the local-first meeting recorder that today runs only on Apple
Silicon macOS, to Windows 11 so the Product Owner can use it on the work
laptop (RTX 3060 Ti 8 GB) next to Microsoft Teams desktop and Google Meet in
Chrome. Same product thesis, unchanged: record the microphone ("Me") and the
system audio (the other side of the call) as two tracks from the OS audio
stack, transcribe Italian and English on-device, separate the remote speakers,
and expose the library to the user's own Claude over the local MCP bridge for
summaries. No bot, no meeting-platform API, no reading of the meeting client
UI, no cloud. Audio never leaves the machine.

Everything that is already cross-platform in the fork (Tauri shell, tray,
session manager, mixing, finalize, crash recovery, `drop_bleed`, SQLite
history, transcribe-cpp ASR with the Vulkan backend, Sortformer diarization,
MCP server, auto-capture decider) is reused as is. Only the two macOS-only
components get a Windows implementation: system-audio capture and the
per-process output sensor. Plus the Windows build and packaging.

## Users
One user: the Product Owner, on Windows 11 with an NVIDIA GPU, several Teams
and Meet meetings a week in Italian and English. The Mac build keeps working
unchanged; nothing in this project may regress it.

## Success criteria
- W1 `bun tauri dev` and `bun tauri build` succeed on the reference laptop
  from a clean clone following `handy/BUILD.md`; the NSIS installer installs
  and launches.
- W2 A meeting session started from the tray records both tracks on Windows:
  mic and system audio, each landing as its own PCM, mixed into one WAV, with
  the system track aligned to the mic track within 200 ms over a 30-minute
  call (silence on the loopback must not shorten the track).
- W3 Transcription of a real Italian Teams call with Nemotron Streaming 3.5
  on Vulkan completes faster than 0.2x the recording length; English and
  mixed IT/EN calls produce transcripts the PO judges usable.
- W4 Sortformer diarization on the system track finds the right number of
  remote speakers on 3 real calls with 2 to 4 remote participants; the mic
  track is always "Me".
- W5 Auto-capture, when enabled, starts a session within 3 s of Teams or
  Chrome starting to play audio, never triggers on Plaudy's own playback, and
  ends on captured silence exactly as on macOS.
- W6 The MCP server finds `history.db` on Windows with no configuration and
  Claude Code can `list_sessions` / `get_session` / `search_sessions` from the
  repo's `.mcp.json`.
- W7 `cargo test --lib` stays green on Windows and macOS; the macOS bundle
  check `scripts/verify-bundle.sh` still passes; a Windows equivalent checks
  every shipped DLL resolves.
- W8 Zero network use during capture, transcription and diarization,
  verified with the firewall log or Resource Monitor during a test session.

## Non-goals
- Live (during-the-meeting) transcription. Plaudy finalizes at stop; that
  stays.
- Reading Teams or Meet UI, DOM or accessibility tree. Audio is the only
  input.
- Cloud ASR or cloud summaries. The MCP bridge sends transcript text only to
  the Claude the user already runs, on demand.
- A Claude API key or any paid summarizer inside the app.
- Windows on ARM, Windows 10 below 2004, Linux (Handy upstream covers Linux
  builds; not validated here).
- Dictation / push-to-talk regressions: Handy upstream already ships them on
  Windows; we do not touch that path except through the shared capture seam.
- Cross-meeting voice profiles ("recognise Marco from last week") are a
  separate later story (PROF-001), not part of the port.
