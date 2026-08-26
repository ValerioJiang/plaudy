# Diarization: why the speaker count was wrong, and what replaced the engine

Session of 2026-08-26. Companion to [HANDOFF.md](HANDOFF.md); the decision trail with timestamps
is in [`.audit/realign-handy-0.9.6.tsv`](../.audit/realign-handy-0.9.6.tsv).

## The complaint

"Non riesce a riconoscere correttamente quanti speaker ci sono." Every diarized row in the
library reported a speaker count that had little to do with how many people were in the room.

## What was actually broken

Measured, not inferred. Two rows of the real `history.db` were re-diarized through the
production `diarize()` with the `rediarize_recording` harness (it reuses the production path, so
calibrating there calibrates production):

| row | length | sherpa @ threshold 0.7 | time |
|---|---|---|---|
| 57 | 35.1 min | **12 speakers** | 343 s (6.1× real time) |
| 63 |  9.3 min | 4 speakers | 86 s (6.5× real time) |

Row 63 turned out **not** to be a pure bug: reading the transcript around 194.6 s shows the audio
switching from an Italian narrator to an English one — two real people. Its true count is 2, so
sherpa's 4 is "2 real + 2 crumbs", not "1 person shattered into 4". Worth stating, because the
code alone would have led to the wrong conclusion.

Row 57 is the real failure, and it has two distinct causes:

1. **Crumb speakers.** Six of the twelve had exactly *one* segment, 3–12 s of talk time each.
   Agglomerative clustering over speaker embeddings shatters short turns, and no cosine threshold
   fixes that without fusing real people (0.7 was already raised from sherpa's 0.5 after the
   89-min → 104-speaker incident).
2. **Identity lost at a window seam.** Above `DIAR_WINDOW_THRESHOLD_SECS` (20 min) the track is
   diarized in 10-minute windows and re-stitched by voting on the overlap. In row 57 Speaker 1
   ends at 1161 s and Speaker 10 appears at 1206 s — the same person, new id. `stitch_windows`
   already documented why this is unfixable in place: sherpa's Rust API exposes only
   `{start, end, speaker}` per segment, never the cluster vectors, so cross-window re-identification
   has nothing to compare.

Neither is a tuning problem. Both are properties of the engine.

## What replaced it

The realign to Handy v0.9.6 brought `transcribe-cpp` 0.2, which has a native **`sortformer`**
family. `handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf` (NVIDIA Streaming Sortformer
4spk v2.1, 139 MB at Q8_0) is not in the bundled catalog — upstream excludes models that produce
no text — but the library runs it, on Metal.

It is an end-to-end neural diarizer: it emits speaker activity directly. No embeddings, so no
clustering threshold to over-split; it streams internally, so it carries speaker identity across
a whole recording and needs no windowing. **Both root causes stop existing rather than getting
better.**

The other native option, `moss-transcribe-diarize`, was ruled out in five minutes: English and
Chinese only, so useless for Italian.

### Measured

Same `align`, same report, both engines called through their production functions
(`diarizer_ab`, an `--ignored` bench):

| audio | truth | sherpa | Sortformer |
|---|---|---|---|
| row 57, 35.1 min | — | 12 speakers · 343 s | **3 speakers** · 36 s |
| row 63, 9.3 min | 2 | 4 speakers · 86 s | 3 speakers · 9.8 s |
| 3-host podcast, 55.5 min | 3 | **36 speakers** · 583 s | **3 real** (41/36/23%) + one 6.6 s crumb · 56 s |
| All-In podcast, 90.9 min | 4 | **88 speakers** · 914 s | **exactly 4** (32/24/23/21%) · 92 s |

Roughly **10× faster** as well: ~58× real time against sherpa's ~6×.

The two podcasts are the clearest statement of the defect, and they were measured last: on audio
whose true speaker count is 3 and 4, sherpa reports **36** and **88**. In both, everything past
the first handful of speakers is a crumb — on the 3-host podcast, speakers #16 through #36 hold a
few seconds each and round to 0 % of the talk time. This is the row-57 pathology unbounded by
length, and it is why no threshold was ever going to be the fix: 88 clusters is not a calibration
that drifted, it is an engine solving a different problem than the one asked of it.

### The price

The checkpoint is `4spk`: **at most four speakers**, baked in. A five-person meeting gets folded
onto four voices. Set against 2 people becoming 12, that is the better failure mode — but it is a
real ceiling and it is why sherpa was kept rather than deleted.

## How it is wired

`DiarizationManager` now holds both engines and picks by what is installed
(`managers/diarization.rs`):

- `sortformer.gguf` present → Sortformer. Otherwise the sherpa ONNX pair. Either set alone is
  enough, so an install that predates this change keeps diarizing with no regression.
- `HANDY_DIAR_ENGINE=sherpa|sortformer` forces one, and is the escape hatch for a recording with
  more than four speakers. An unknown value or a missing model logs a warning and falls back to
  whatever is installed, rather than silently disabling diarization.
- Any Sortformer failure (load, session, run) logs and falls through to sherpa. A recording with
  an unattributed transcript is still useful; a lost recording is not.
- `sortformer_turns` drops segments whose timestamps are both zero — the C ABI's "attributed but
  untimed" signal, which kept as a turn would sit at t=0 and win the overlap vote for the
  recording's opening words.

Everything downstream — `align`, `merge_segments`, `drop_bleed`, the History timeline — is
untouched. The engine returns the same `Vec<SpeakerTurn>` either way.

`HANDY_DIAR_THRESHOLD`, `HANDY_DIAR_NUM_CLUSTERS` and `HANDY_DIAR_MAX_SPEAKERS` still exist and
still apply, but only to the sherpa path.

## How the model reaches the app

`scripts/fetch-diarization-model.sh` — pinned revision, sha256-verified, promoted into place only
after it verifies (so an interrupted fetch never leaves half a model for the app to load). It
writes to `handy/src-tauri/resources/models/diarization/sortformer.gguf`, which `tauri build`
bundles and `migrate_bundled_diarization_models` installs on first run alongside the ONNX pair.

**The file is gitignored.** At 139 MB it is over GitHub's 100 MB per-file hard limit, and a model
checkpoint is a build input rather than source. Run the script once per clone and before any
release build. Skipping it still produces a working app — it just falls back to sherpa.

`download_diarization_models` (the in-app "Enable diarization" path) deliberately still fetches
only the sherpa pair: that downloader has no resume and no cancel, which is acceptable for two
files under 40 MB and not for one at 139 MB.

## Still open

- **The >4-speaker policy.** Today the only way to reach sherpa on a five-person meeting is the
  env var. Whether that deserves a real setting, an automatic "Sortformer saturated at 4, retry
  with sherpa" heuristic, or nothing at all, is a product call that wants more real meetings
  behind it.
- **Rows already in the library** keep their old, wrong attribution. `--rediarize <id>` re-runs
  one row through the new engine; nothing re-diarizes the whole library.
- **Sortformer on a real multi-person call in a room** (not system audio from a video) has not
  been measured. Every result above is either a podcast or a screen recording.
- Sortformer makes the sherpa path dead weight for most users. If the >4 case is judged not worth
  supporting, dropping `sherpa-onnx` removes ~29 MB of dylibs from the bundle and ends the
  `@rpath` / dual-ONNX-runtime juggling in `build.rs` and `.cargo/config.toml`.

## Re-running the measurements

```bash
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$HOME/.bun/bin:$PATH"
export CMAKE_POLICY_VERSION_MINIMUM=3.5
D="$HOME/Library/Application Support/com.uppify.plaudy"
cd handy/src-tauri

# Both engines over one WAV whose true speaker count you know.
HANDY_DIAR_MODELS="$D/models" HANDY_AB_WAV=/path/to/audio.wav HANDY_AB_TRUTH=4 \
  cargo test --lib diarizer_ab -- --ignored --nocapture
# HANDY_AB_ENGINE=sortformer|sherpa runs only one side (sherpa is ~10x slower).

# One existing History row, through the production path, read-only.
HANDY_REDIAR_WAV="$D/recordings/<file>.wav" HANDY_REDIAR_DB="$D/history.db" \
HANDY_REDIAR_MODELS="$D/models" HANDY_REDIAR_HISTORY_ID=57 \
  cargo test --lib rediarize_recording -- --ignored --nocapture
```

Both benches need a 16 kHz mono 16-bit WAV and assert on it rather than guessing.
