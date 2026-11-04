//! Windows "is some OTHER app playing audio?" sensor for seamless auto-capture (IF-W02).
//!
//! FRZ-W01 lands this as a stub that never triggers; WIN-SENSE-001 fills `list_process_output`
//! with the WASAPI audio-session enumeration (`IAudioSessionManager2` on the default render
//! endpoint, own PID excluded, peak-meter gated). The trigger decision `any_external_output` is
//! the same pure function as the macOS sensor's, mirrored here with the same four tests rather
//! than moved, so the macOS file stays byte-identical (FRZ-W01 rule).
#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
#![allow(dead_code)]

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

/// Snapshot of every process's (pid, is-running-output). Stub: empty, so auto-capture never
/// triggers from system audio on Windows until WIN-SENSE-001. Empty is also the error default.
fn list_process_output() -> Vec<ProcessAudio> {
    Vec::new()
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
    fn stub_sensor_never_triggers() {
        // FRZ-W01 contract: until WIN-SENSE-001, the Windows sensor is inert.
        assert!(!external_output_active());
    }
}
