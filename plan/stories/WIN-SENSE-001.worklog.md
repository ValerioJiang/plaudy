# WIN-SENSE-001 — worklog

Written for someone who has never seen the conversation that produced this
story. Update it whenever you finish a step, hit a dead end, or get blocked,
and commit it with the code.

## State
Started 2026-11-04T12:20Z on branch `WIN-SENSE-001` in the main checkout
(no worktree: a worktree means a second 5-minute full build; the branch is
the isolation). Done 2026-11-04T12:50Z, **self-reported**. Three criteria
need the PO (silent-machine run, supervisor E2E on a Teams test call,
Chrome-silent-tab check): Chrome was playing audio and Teams was open on the
laptop for the whole implementation window, so a trustworthy run of those
was not possible from this session (see "Blocked on").

## Done
- `audio/output_sensor_windows.rs`: the stub is now the real sensor.
  - `snapshot()`: `ensure_com()` initialises COM (MTA) **once per calling
    thread** through a thread-local `ComGuard`, released at thread exit
    (owns the uninit only when the init succeeded; `RPC_E_CHANGED_MODE`
    tolerated) → `IMMDeviceEnumerator::EnumAudioEndpoints(eRender,
    DEVICE_STATE_ACTIVE)` → per endpoint `IAudioSessionManager2` →
    `IAudioSessionEnumerator` → per session `IAudioSessionControl2::
    GetProcessId()` + `GetState()`, and for *active* sessions
    `IAudioMeterInformation::GetPeakValue()`. Every interface is a `windows`
    smart pointer released on drop.
  - Decision: `session_is_running_output(active, peak) = active && peak >
    PEAK_FLOOR` with `PEAK_FLOOR = 0.001` (-60 dBFS, documented in the
    file); `any_external_output()` unchanged (own PID excluded). PID 0
    (system sounds) counts like any other process.
  - Errors: any `HRESULT` failure → empty list → `false`; logged at debug
    **once per distinct error** via a `LAST_ERROR` mutex, plus one
    "recovered" line when it clears.
  - **Deviation from the spec text, flagged FYI in `plan/ATTENTION.md`:**
    the spec says "default render endpoint"; the implementation enumerates
    **every active render endpoint**, for the same reason WIN-CAP-003 does:
    a Teams call on the WH-1000XM4 plays through the Hands-Free endpoint,
    which is not the console default, and would never have triggered.
    Cost: one `Activate(IAudioSessionManager2)` per endpoint per tick
    (3 on this laptop) at 4 Hz; the soak below shows it is nothing.
- No `Cargo.toml` change: `IAudioMeterInformation` lives under
  `Win32_Media_Audio_Endpoints`, already enabled.
- Tests (`cargo test --lib output_sensor`): **9 passed, 3 ignored**.
  - The four mirrored decision tests, three `session_is_running_output`
    tests, one error-dedupe test, and `live_snapshot_never_errors_on_this_
    machine` (not ignored: a plain enumeration must never error on a machine
    with a render endpoint; it printed 4 sessions: 3× PID 0 inactive, one
    per endpoint, and Chrome PID 25612 active, peak 0.98).
  - `live_external_powershell_sound_triggers` (ignored, run by hand): the
    assertion is keyed on the **spawned PowerShell's PID**, not on the
    global verdict, because Chrome was already playing and would have made
    the plain version pass vacuously (first cut did: "seen after 7.9 ms").
    Result: `powershell pid 31232 seen playing: Some((321.65ms,
    SessionSample { pid: 31232, active: true, peak: 0.0073 }, true))`
    → **true within 322 ms** (spec asks < 1 s).
  - `live_silent_machine_reads_false_for_five_seconds` (ignored): **ran and
    failed as expected on a non-silent machine**: tick 0 listed
    `pid 25612 (chrome.exe) active, peak 0.98`. The test prints the raw
    sessions with peaks on failure so the culprit is named. Needs the PO's
    silent laptop.
  - `live_soak_four_hz_is_flat_in_memory` (ignored): see "Soak" below.
- **Soak found a leak, fixed.** `live_soak_four_hz_is_flat_in_memory`
  samples working set and private bytes every 30 s; env knobs
  `PLAUDY_SENSOR_SOAK_SECS` (600), `PLAUDY_SENSOR_SOAK_TICK_MS` (250) and
  `PLAUDY_SENSOR_SOAK_MODE` (`sensor`, `noop`, `endpoints`, suffix `-held`
  keeps one COM init for the loop).
  - First 10-minute 4 Hz run with a per-call init/uninit guard: working
    set 13.2 → 18.4 MiB, steady climb (~2 KB per tick).
  - Bisection at 50 Hz for 90-150 s (private bytes, MiB at 0 s → 90 s):
    `noop` 2.6 → 2.4 (control flat); `sensor` 2.8 → 9.4; `endpoints`
    (COM init + `MMDeviceEnumerator` + `EnumAudioEndpoints` only, no
    sessions) 2.7 → 9.5; `endpoints-held` 2.7 → 2.9; `sensor-held` 2.9 →
    3.2 flat after the first 30 s.
  - Root cause: when the call's `CoUninitialize` is the thread's last, COM
    tears the apartment down and the audio stack leaks per cycle. Session
    enumeration and meters are not the leak.
  - Fix: thread-local COM init (above). After the fix, `sensor` at 50 Hz:
    private 2.8 → 3.2 → 3.1 → 3.2 → 3.2 MiB over 5 353 ticks (flat).
    The spec's 10-minute 4 Hz run (2 247 ticks): working set 14.1 → 14.8
    MiB, private 2.8 → 3.3 MiB, plateau from t=300 s on (3.2-3.3). **Flat.**

- `cargo test --lib`: **389 passed, 0 failed, 7 ignored** (was 385/4; +4
  unit tests, +3 ignored live tests in this module).

## In flight
Nothing half-changed. Files touched: `output_sensor_windows.rs`, the spec
(checkboxes), this worklog, `plan/BACKLOG.md`, `plan/ATTENTION.md`,
`plan/HANDOFF.md`.

## Tried and rejected
- Per-call `CoInitializeEx`/`CoUninitialize` (the spec's literal wording
  "initialised per call"): rejected after the soak, leaks ~2 KB per tick
  (see Done). The spec's "if not already" is honoured by the thread-local.
- Default render endpoint only (spec text): rejected, misses the Hands-Free
  endpoint; see Deviation above.
- `IAudioSessionManager2` + `IAudioSessionNotification` callbacks instead
  of enumerating per tick: rejected, the supervisor is already a 4 Hz poll
  and the meter value has to be read per tick anyway; callbacks would add a
  COM object lifetime for no gain.
- Caching the enumerator / session managers across ticks: rejected for v1,
  a cached manager goes stale when the endpoint set changes (earphones),
  and the per-tick cost is microseconds.
- Reading the process name for the log: rejected, needs `Win32_System_
  ProcessStatus` or `Win32_System_Threading` OpenProcess calls on every PID
  per tick; the PID is enough to debug with `Get-Process -Id`.
- Running the supervisor E2E from this session: rejected on purpose.
  Teams was open and Chrome was playing audio on the PO's laptop; turning
  auto-capture on would have recorded whatever was happening without the
  PO watching. Consent before convenience.

## Blocked on
PO, on a quiet laptop (close Chrome's playing tab, no call in progress):
1. `cargo test --lib output_sensor -- --ignored --nocapture` with nothing
   playing: `live_silent_machine_reads_false_for_five_seconds` must print
   `20 ticks, all false`; if it fails it prints the offending PID and peak,
   `Get-Process -Id <pid>` names the app. Paste the line here.
2. Known-limitation check: Chrome open on a silent tab (a paused YouTube
   video keeps the session *Active* with peak 0.0), VS Code open, then run
   the same test: must still read false. That is the meter gate doing its
   job; note the peak values printed.
3. Supervisor E2E: in the app, Settings → Sessions → Auto-capture ON (the
   setting is off by default; the description still says "this Mac",
   frontend wording is out of scope here). Start a Teams test call (Teams'
   "Make a test call"): expect in `%LOCALAPPDATA%\com.uppify.plaudy\logs\
   plaudy.log` `Auto-capture sensors: external_output = true` then within
   `START_AFTER` 1.2 s + 1 s `Auto-capture: audio detected → session
   started.`; hang up: `speakers quiet → session finalized.` about
   `STOP_AFTER` 4 s later. Then play that recording back in the app's
   History: no new `session started` line (the `playback_active()` mask).
   Paste the four timestamps here. Turn the setting back off if you do not
   want it on.

## Next action
PO runs the three checks above. Verifier reviews against the spec. Commit
(suggested message start: `WIN-SENSE-001:`), merge, then WIN-BUILD-001.
