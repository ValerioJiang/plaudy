# Attention queue

Newest first. Clear a line when you have dealt with it.

- [ ] **DECIDE** `ASSUMPTION` WIN-CAP-001 and FRZ-W01 were implemented in the main checkout with no worktree (plan files uncommitted). Assumption: you commit the scaffold plus the three stories now so Wave 1 can use worktrees. — 2026-11-04T09:09:46Z _(main)_
- [ ] **DECIDE** `OPTIONS` Diarization on Windows: Sortformer is compiled out by the macOS-aarch64 gate in diarization.rs (:298, :316, :444) although transcribe-cpp is built here; sherpa is not a Windows dependency at all. PM pick: a small story putting diarize/diarize_sortformer under a predicate true on macOS-aarch64 and Windows-x86_64, sherpa stays macOS-only. The earlier Sortformer-vs-sherpa item is moot as posed. — 2026-11-04T09:09:46Z _(main)_
- [ ] **DECIDE** `OPTIONS` WIN-BUILD-000 vs missing Vulkan SDK: the spec's CPU fallback 'by cargo feature flags only' is impossible (vulkan is hard-wired in the cfg(windows,x86_64) transcribe-cpp dep). PM pick: install the SDK first, drop the fallback clause (Vulkan failure = report + stop). Alt: allow a throwaway uncommitted Cargo.toml edit in the worktree. — 2026-11-03T17:00:43Z _(main)_
- [ ] **DECIDE** `ASSUMPTION` FRZ-W01 needs a macOS regression run (cargo test --lib + scripts/verify-bundle.sh) before merge. Assumption: you still have the Mac and will run it when asked. — 2026-11-03T16:42:30Z _(main)_
- [ ] **DECIDE** `OPTIONS` Diarization on Windows v1: Sortformer only (4-speaker ceiling, no onnxruntime DLL to ship) vs also shipping the sherpa fallback for larger calls. PM pick: Sortformer only for v1, sherpa behind HANDY_DIAR_ENGINE if the baseline shows it just works. — 2026-11-03T16:42:30Z _(main)_
- [ ] **DECIDE** `RISK-NEW` Consent and retention on the work laptop: tray ear is the visible signal; PO announces recording in call; set history retention for raw audio. Confirm the retention period (PM pick: 7 days for WAVs, transcripts kept). — 2026-11-03T16:42:30Z _(main)_
