//! Windows "is some OTHER app playing audio?" sensor for seamless auto-capture (IF-W02,
//! WIN-SENSE-001).
//!
//! Enumerates the WASAPI **audio sessions** on every active render endpoint
//! (`IAudioSessionManager2` → `IAudioSessionEnumerator`), reads each session's owning PID and
//! state (`IAudioSessionControl2`) and its per-session **peak meter**
//! (`IAudioMeterInformation`, the same numbers the Volume Mixer shows), and reports a process
//! as "running output" only when its session is `AudioSessionStateActive` *and* the meter is
//! above [`PEAK_FLOOR`]. Plain reads: no capture stream, no permission, no recording indicator.
//!
//! Why the meter gate: on Windows an app's session goes *Active* as soon as it holds an open
//! render stream, which Chrome does for a silent tab and Teams does while idle in the lobby;
//! without the meter, both would read as "someone is playing audio" and auto-capture would
//! open junk sessions. The meter says whether samples are actually flowing right now.
//!
//! Why every active endpoint and not the default one: the same reason WIN-CAP-003 captures
//! them all. A Teams call on Bluetooth earphones plays through the *Hands-Free* endpoint,
//! which is often not the console default, and would otherwise never trigger.
//!
//! Own-PID exclusion mirrors the macOS sensor (read `output_sensor.rs` for the v1 post-mortem
//! that made per-process attribution the rule). Note that under Tauri our WebView2 renderer is
//! a *separate* process, so audio played by the in-app player is not caught by the PID filter;
//! that case is masked by `playback_active()` in `audio/mod.rs`, which stays there (IF-W02).
//!
//! All `unsafe` COM is confined to this file; the trigger decision itself is pure and
//! unit-tested, mirrored from the macOS file rather than shared so that file stays untouched.
#![cfg(all(target_os = "windows", target_arch = "x86_64"))]

use std::sync::Mutex;

use windows::core::Interface;
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::Audio::Endpoints::IAudioMeterInformation;
use windows::Win32::Media::Audio::{
    eRender, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// A session counts as emitting audio only when its peak meter (0.0..=1.0, full scale) is above
/// this. 0.001 is -60 dBFS: well below any speech or music, well above the meter's idle reading
/// of exactly 0.0 for an open-but-silent stream (Chrome silent tab, Teams lobby, VS Code).
const PEAK_FLOOR: f32 = 0.001;

/// One audio-emitting process's output state, as the sensor sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProcessAudio {
    pid: i32,
    running_output: bool,
}

/// The pure trigger decision: is any process other than us emitting audio?
fn any_external_output(processes: &[ProcessAudio], own_pid: i32) -> bool {
    processes
        .iter()
        .any(|p| p.running_output && p.pid != own_pid)
}

/// The pure per-session rule: active *and* audible. Kept separate so it is unit-testable
/// without COM.
fn session_is_running_output(active: bool, peak: f32) -> bool {
    active && peak > PEAK_FLOOR
}

/// Last COM/WASAPI error we logged, so a persistent failure costs one debug line, not four a
/// second (the supervisor polls at 4 Hz).
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

fn note_error(err: String) {
    let mut last = LAST_ERROR.lock().unwrap_or_else(|p| p.into_inner());
    if last.as_deref() != Some(err.as_str()) {
        log::debug!("output sensor: {err} (sensor reads false until this clears)");
        *last = Some(err);
    }
}

fn clear_error() {
    let mut last = LAST_ERROR.lock().unwrap_or_else(|p| p.into_inner());
    if last.is_some() {
        log::debug!("output sensor: recovered");
        *last = None;
    }
}

/// COM apartment guard. `RPC_E_CHANGED_MODE` (the thread is already an STA, e.g. a Tauri
/// thread) means COM is usable but not ours to uninitialise.
struct ComGuard {
    owned: bool,
}

impl ComGuard {
    fn enter() -> Result<Self, String> {
        // SAFETY: plain COM apartment bookkeeping on the calling thread.
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if hr.is_ok() {
            Ok(Self { owned: true })
        } else if hr == RPC_E_CHANGED_MODE {
            Ok(Self { owned: false })
        } else {
            Err(format!("CoInitializeEx failed: {hr}"))
        }
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.owned {
            // SAFETY: balances the successful CoInitializeEx in `enter`.
            unsafe { CoUninitialize() };
        }
    }
}

thread_local! {
    /// The calling thread's COM init, entered on first use and released when the thread exits.
    ///
    /// Why not init/uninit per call: when the call's uninit is the thread's *last*, COM tears
    /// the apartment down and the audio stack (MMDevAPI) leaks ~2 KB each cycle. Measured with
    /// `live_soak_four_hz_is_flat_in_memory` at 50 Hz: per-call guard +2.3 MiB private per
    /// 30 s, thread-held guard flat (+0.0). The supervisor polls from one long-lived thread, so
    /// one init for its lifetime is exactly right.
    static THREAD_COM: std::cell::RefCell<Option<ComGuard>> = const { std::cell::RefCell::new(None) };
}

/// Make sure COM is initialised on the calling thread (once; kept until the thread exits).
fn ensure_com() -> Result<(), String> {
    THREAD_COM.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(ComGuard::enter()?);
        }
        Ok(())
    })
}

/// One audio session as read from WASAPI, before the decision rule is applied. Kept for the
/// live tests, which want to see the meter values behind a verdict.
#[derive(Debug, Clone, Copy, PartialEq)]
struct SessionSample {
    pid: i32,
    active: bool,
    peak: f32,
}

/// Snapshot of every audio session's (pid, is-running-output) across all active render
/// endpoints. Empty on any error: the safe default, auto-capture simply never triggers.
/// One process with several sessions (Chrome, Teams) yields several entries; `any` is what we
/// ask of the list, so that is harmless.
fn list_process_output() -> Vec<ProcessAudio> {
    sessions()
        .iter()
        .map(|s| ProcessAudio {
            pid: s.pid,
            running_output: session_is_running_output(s.active, s.peak),
        })
        .collect()
}

/// Every session on every active render endpoint, or an empty list after logging the error
/// (once per distinct error).
fn sessions() -> Vec<SessionSample> {
    match snapshot() {
        Ok(list) => {
            clear_error();
            list
        }
        Err(e) => {
            note_error(e);
            Vec::new()
        }
    }
}

fn snapshot() -> Result<Vec<SessionSample>, String> {
    ensure_com()?;
    // SAFETY: every interface below is a COM smart pointer released on drop; no raw memory
    // crosses the boundary.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator) failed: {e}"))?;
        let devices = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|e| format!("EnumAudioEndpoints failed: {e}"))?;
        let count = devices
            .GetCount()
            .map_err(|e| format!("IMMDeviceCollection::GetCount failed: {e}"))?;
        let mut out = Vec::new();
        for i in 0..count {
            let device = devices
                .Item(i)
                .map_err(|e| format!("IMMDeviceCollection::Item({i}) failed: {e}"))?;
            device_sessions(&device, &mut out)?;
        }
        Ok(out)
    }
}

/// Append one endpoint's sessions to `out`.
unsafe fn device_sessions(device: &IMMDevice, out: &mut Vec<SessionSample>) -> Result<(), String> {
    let manager: IAudioSessionManager2 = device
        .Activate(CLSCTX_ALL, None)
        .map_err(|e| format!("IMMDevice::Activate(IAudioSessionManager2) failed: {e}"))?;
    let sessions = manager
        .GetSessionEnumerator()
        .map_err(|e| format!("GetSessionEnumerator failed: {e}"))?;
    let count = sessions
        .GetCount()
        .map_err(|e| format!("IAudioSessionEnumerator::GetCount failed: {e}"))?;
    for i in 0..count {
        let control = sessions
            .GetSession(i)
            .map_err(|e| format!("IAudioSessionEnumerator::GetSession({i}) failed: {e}"))?;
        let control2: IAudioSessionControl2 = control
            .cast()
            .map_err(|e| format!("IAudioSessionControl2 cast failed: {e}"))?;
        // PID 0 is the system-sounds session; it counts like any other process (a Windows
        // notification chime does play audio, and the supervisor's debounce absorbs it).
        let pid = control2
            .GetProcessId()
            .map_err(|e| format!("GetProcessId failed: {e}"))? as i32;
        let active = control2
            .GetState()
            .map_err(|e| format!("IAudioSessionControl::GetState failed: {e}"))?
            == AudioSessionStateActive;
        // Only meter the active sessions: an inactive/expired session is silent by definition.
        let peak = if active {
            let meter: IAudioMeterInformation = control
                .cast()
                .map_err(|e| format!("IAudioMeterInformation cast failed: {e}"))?;
            meter
                .GetPeakValue()
                .map_err(|e| format!("GetPeakValue failed: {e}"))?
        } else {
            0.0
        };
        out.push(SessionSample { pid, active, peak });
    }
    Ok(())
}

/// True if any app *other than us* is currently emitting audio. False on any error.
pub fn external_output_active() -> bool {
    any_external_output(&list_process_output(), std::process::id() as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(pid: i32, running_output: bool) -> ProcessAudio {
        ProcessAudio {
            pid,
            running_output,
        }
    }

    const OWN: i32 = 42;

    #[test]
    fn no_processes_means_no_trigger() {
        assert!(!any_external_output(&[], OWN));
    }

    #[test]
    fn our_own_output_never_triggers() {
        assert!(!any_external_output(&[p(OWN, true)], OWN));
    }

    #[test]
    fn a_silent_bystander_does_not_trigger() {
        assert!(!any_external_output(&[p(OWN, true), p(7, false)], OWN));
    }

    #[test]
    fn another_app_emitting_audio_triggers() {
        assert!(any_external_output(&[p(OWN, true), p(7, true)], OWN));
    }

    #[test]
    fn an_active_but_silent_session_is_not_running_output() {
        // Chrome on a silent tab, Teams idle in the lobby: state Active, meter 0.0.
        assert!(!session_is_running_output(true, 0.0));
        assert!(!session_is_running_output(true, PEAK_FLOOR));
    }

    #[test]
    fn an_inactive_session_is_never_running_output_whatever_the_meter_says() {
        assert!(!session_is_running_output(false, 0.5));
    }

    #[test]
    fn an_active_audible_session_is_running_output() {
        assert!(session_is_running_output(true, 0.002));
        assert!(session_is_running_output(true, 1.0));
    }

    #[test]
    fn error_log_is_once_per_distinct_error() {
        // The dedupe is what keeps a dead audio service from logging 4x/s; exercise the state
        // machine directly (the log output itself is not asserted).
        *LAST_ERROR.lock().unwrap() = None;
        note_error("a".into());
        assert_eq!(LAST_ERROR.lock().unwrap().as_deref(), Some("a"));
        note_error("a".into());
        note_error("b".into());
        assert_eq!(LAST_ERROR.lock().unwrap().as_deref(), Some("b"));
        clear_error();
        assert_eq!(*LAST_ERROR.lock().unwrap(), None);
    }

    #[test]
    fn live_snapshot_never_errors_on_this_machine() {
        // Not ignored: enumerating sessions is harmless and must not fail on a machine with
        // any active render endpoint. Also pins the COM guard to a plain test thread.
        let list = sessions();
        eprintln!("audio sessions: {list:?}");
        assert!(
            LAST_ERROR.lock().unwrap().is_none(),
            "snapshot reported an error"
        );
    }

    // ----- live acceptance (manual) ------------------------------------------------------
    // Run: cargo test --lib output_sensor -- --ignored --nocapture

    /// Negative path: a silent machine must read `false` for 5 s at the supervisor's 4 Hz.
    /// Close anything that plays audio first (a browser tab with a paused video is fine).
    #[test]
    #[ignore = "manual: needs live WASAPI; machine must be silent"]
    fn live_silent_machine_reads_false_for_five_seconds() {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut ticks = 0u32;
        while std::time::Instant::now() < deadline {
            let raw = sessions();
            let active = any_external_output(&list_process_output(), std::process::id() as i32);
            if active {
                eprintln!("tick {ticks}: sessions = {raw:?}");
            }
            assert!(
                !active,
                "silent machine read as external audio at tick {ticks}"
            );
            ticks += 1;
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        eprintln!("{ticks} ticks, all false");
    }

    /// Positive path: another process (PowerShell playing a system WAV) must trigger the sensor
    /// within 5 s. The assertion is keyed on *that child's PID*, so a browser already playing
    /// in the background cannot make the test pass by itself. Poll-until-timeout: SoundPlayer
    /// takes a moment to open the endpoint.
    #[test]
    #[ignore = "manual: needs live WASAPI + an audio output device"]
    fn live_external_powershell_sound_triggers() {
        let script = "$p = New-Object Media.SoundPlayer 'C:/Windows/Media/tada.wav'; \
                      1..3 | ForEach-Object { $p.PlaySync() }";
        let mut child = std::process::Command::new("powershell")
            .args(["-NoProfile", "-c", script])
            .spawn()
            .expect("spawn powershell");
        let child_pid = child.id() as i32;

        let started = std::time::Instant::now();
        let deadline = started + std::time::Duration::from_secs(5);
        let mut seen = None;
        while std::time::Instant::now() < deadline {
            let raw = sessions();
            if let Some(s) = raw
                .iter()
                .find(|s| s.pid == child_pid && session_is_running_output(s.active, s.peak))
            {
                seen = Some((started.elapsed(), *s, external_output_active()));
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        let _ = child.kill();
        let _ = child.wait();
        eprintln!("powershell pid {child_pid} seen playing: {seen:?}");
        let (_, _, sensor) =
            seen.expect("tada.wav must show as the child's active, audible session within 5 s");
        assert!(sensor, "sensor must read true while the child plays");
    }

    /// Soak: the supervisor's 4 Hz poll for `PLAUDY_SENSOR_SOAK_SECS` seconds (default 600),
    /// printing the working set every 30 s so a leak in the COM path would show as growth.
    #[test]
    #[ignore = "manual: 10-minute soak; set PLAUDY_SENSOR_SOAK_SECS to shorten"]
    fn live_soak_four_hz_is_flat_in_memory() {
        let env_u64 = |k: &str, d: u64| {
            std::env::var(k)
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(d)
        };
        let secs = env_u64("PLAUDY_SENSOR_SOAK_SECS", 600);
        let tick_ms = env_u64("PLAUDY_SENSOR_SOAK_TICK_MS", 250);
        // Diagnostic modes: `sensor` (default, what the supervisor calls), `noop` (control:
        // the measurement itself), `endpoints` (COM + endpoint enumeration only).
        let mode = std::env::var("PLAUDY_SENSOR_SOAK_MODE").unwrap_or_else(|_| "sensor".into());
        eprintln!("soak: mode={mode} secs={secs} tick={tick_ms}ms");
        // `*-held`: keep one COM init alive for the whole loop, as a long-lived thread would.
        let _held = mode.ends_with("-held").then(|| ComGuard::enter().unwrap());
        let mode = mode.trim_end_matches("-held").to_string();
        let started = std::time::Instant::now();
        let mut next_report = std::time::Duration::ZERO;
        let mut ticks = 0u64;
        while started.elapsed() < std::time::Duration::from_secs(secs) {
            match mode.as_str() {
                "noop" => {}
                "endpoints" => {
                    let _com = ComGuard::enter().unwrap();
                    unsafe {
                        let e: IMMDeviceEnumerator =
                            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
                        let d = e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE).unwrap();
                        let _ = d.GetCount();
                    }
                }
                _ => {
                    let _ = external_output_active();
                }
            }
            ticks += 1;
            if started.elapsed() >= next_report {
                let (ws, pb) = memory_mib().unwrap_or((f64::NAN, f64::NAN));
                eprintln!(
                    "t={:>4}s ticks={ticks} working_set={ws:.1} MiB private={pb:.1} MiB",
                    started.elapsed().as_secs(),
                );
                next_report += std::time::Duration::from_secs(30);
            }
            std::thread::sleep(std::time::Duration::from_millis(tick_ms));
        }
    }

    /// Our own (working set, private bytes) in MiB, via PowerShell so this test needs no extra
    /// `windows` feature. `None` if PowerShell is unavailable.
    fn memory_mib() -> Option<(f64, f64)> {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-c",
                &format!(
                    "$p = Get-Process -Id {}; \"$($p.WorkingSet64) $($p.PrivateMemorySize64)\"",
                    std::process::id()
                ),
            ])
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        let mut it = text.split_whitespace().map(|s| s.parse::<f64>().ok());
        let mib = 1024.0 * 1024.0;
        Some((it.next()??.max(0.0) / mib, it.next()??.max(0.0) / mib))
    }
}
