#!/usr/bin/env bash
# Fetch the Sortformer diarization model into the Tauri resources tree, so `tauri build`
# bundles it and the app diarizes offline from first launch.
#
# The file is NOT committed: at 139 MB it is over GitHub's 100 MB per-file hard limit, and a
# model checkpoint is a build input, not source. This script is the reproducible way to
# materialise it — pinned to a commit sha and verified against its sha256, exactly like the
# in-app catalog downloads.
#
#   scripts/fetch-diarization-model.sh          fetch if missing (verifies an existing file)
#   scripts/fetch-diarization-model.sh --force  re-download even if present
#
# Run it once after cloning, and before any release build.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST_DIR="$REPO_ROOT/handy/src-tauri/resources/models/diarization"
DEST="$DEST_DIR/sortformer.gguf"

# NVIDIA Streaming Sortformer 4spk v2.1, Q8_0. Pinned to a revision so the bytes are immutable.
REPO="handy-computer/diar_streaming_sortformer_4spk-v2.1-gguf"
REVISION="7ef0c15dc8f9d717e9d24fac29a6e6551e9c6ddf"
FILENAME="diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf"
SHA256="a5dacdc650790266c7a362e54e6bf51952015487edaa606c4e11632bc32442a9"
BYTES=139310336

# sha256 of a file. macOS ships `shasum`; Linux and Git Bash on Windows ship `sha256sum`
# (Git Bash has `shasum` only in a login shell, under /usr/bin/core_perl).
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  else
    echo "error: neither sha256sum nor shasum is installed" >&2
    return 1
  fi
}

verify() {
  local file="${1:-$DEST}" actual
  actual="$(sha256_of "$file")" || return 1
  [ "$actual" = "$SHA256" ] || {
    echo "error: sha256 mismatch for $file" >&2
    echo "  expected $SHA256" >&2
    echo "  actual   $actual" >&2
    return 1
  }
}

if [ "${1:-}" != "--force" ] && [ -f "$DEST" ]; then
  if verify; then
    echo "already present and verified: $DEST"
    exit 0
  fi
  echo "re-downloading (existing file failed verification)" >&2
fi

mkdir -p "$DEST_DIR"
TMP="$DEST.part"
echo "fetching $FILENAME ($((BYTES / 1000000)) MB) ..."
curl --fail --location --progress-bar \
  -o "$TMP" \
  "https://huggingface.co/$REPO/resolve/$REVISION/$FILENAME"

# Only promote the file once it verifies, so an interrupted run never leaves a half model
# that the app would try to load.
verify "$TMP" || { rm -f "$TMP"; exit 1; }
mv "$TMP" "$DEST"
echo "ok: $DEST"
