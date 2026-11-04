# Risks (Windows port)

| ID | Risk | Impact | Trigger | Mitigation |
|----|------|--------|---------|------------|
| R1 | WASAPI loopback yields no frames during silence, so the system track is shorter than the mic track and the merged timeline drifts | W2; "who said what" misaligned | Alignment test in WIN-CAP-001 shows > 200 ms skew after 30 min | Fill wall-clock gaps with zeros from a monotonic clock in the capture thread, or keep a silent render stream open; measure both |
| R2 | Vulkan backend of transcribe-cpp fails to build or is slow on this GPU/driver | W1/W3 | WIN-BUILD-000 cannot build `vulkan-shaders-gen` or RTF > 0.5 | Follow BUILD.md path-limit workaround; fall back to CPU `dynamic-backends`; consider CUDA feature if transcribe-cpp exposes it |
| R3 | Sortformer GGUF diarization not validated on Windows/Vulkan (only Metal so far) | W4 | WIN-BUILD-000 shows crash or wrong speaker counts | `HANDY_DIAR_ENGINE=sherpa` fallback (needs onnxruntime DLL next to the exe); report to the macOS maintainer notes |
| R4 | Dual ONNX runtime (sherpa `shared` dylib vs `ort` static) collides differently on Windows DLL loading | Crash at startup | WIN-BUILD-000/001 | Ship sherpa's onnxruntime DLL under its own name/dir; verify with the Windows bundle check (dumpbin / Dependencies) |
| R5 | Mic and loopback opened on different clocks (different devices) drift over an hour | W2 | Alignment test | Resample both into the shared 16 kHz consumer (already done per track); compare lengths at finalize and log skew; `lead_silence_ms` padding exists |
| R6 | Echo of the user's voice in the loopback (Teams plays back nothing of you, but Meet in Chrome with speakers can) | Phantom "Speaker N" that is actually Me | EVAL-W01 | `drop_bleed` already handles mic-hears-system; add the symmetric case if measured |
| R7 | Auto-capture false triggers from Windows system sounds or Spotify | Junk sessions | WIN-SENSE-001 live test | Meter threshold + `START_AFTER` debounce + probation already in the supervisor; optional app allowlist later |
| R8 | Regressing the macOS build while re-gating cfgs | W7 | `cargo test` or `verify-bundle.sh` fails on the Mac | FRZ-W01 keeps macOS code paths identical; two-key verification; PO runs the Mac check before merge |
| R9 | NSIS bundle misses a backend DLL that the dev build finds on PATH | Installer launches a crashing app | WIN-BUILD-001 | Windows analogue of `verify-bundle.sh`: resolve every DLL import of the installed exe and launch for 30 s |
| R10 | Consent: recording colleagues and clients | Legal exposure for the PO | Any meeting | Tray ear indicator already shipped; PO announces recording; raw-audio retention setting (exists in Handy history retention) |
| R11 | Windows mic privacy setting blocks capture silently | No "Me" track | First run | Handy already has `get_windows_microphone_permission_status`; surface it in the Sessions panel |
