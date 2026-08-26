#!/usr/bin/env bash
# Shared setup for the Handy realign tooling. Source it, don't run it.
#
# Materialises two read-only checkouts of upstream Handy as git worktrees:
#   $BASE_DIR  the tag this fork was taken from
#   $THEIRS_DIR the tag we are realigning to
# Everything else in scripts/realign/ diffs and merges against those two trees.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OURS_DIR="$REPO_ROOT/handy"
CENSUS_DIR="$REPO_ROOT/.audit/census"

UPSTREAM_URL="${UPSTREAM_URL:-https://github.com/cjpais/Handy.git}"
BASE_TAG="${BASE_TAG:-v0.8.3}"
THEIRS_TAG="${THEIRS_TAG:-v0.9.6}"

WORK="${REALIGN_WORK:-${TMPDIR:-/tmp}/handy-realign}"
CLONE_DIR="$WORK/upstream"
BASE_DIR="$WORK/$BASE_TAG"
THEIRS_DIR="$WORK/$THEIRS_TAG"

realign_prepare() {
  mkdir -p "$WORK" "$CENSUS_DIR"
  if [ ! -d "$CLONE_DIR/.git" ]; then
    git clone --quiet "$UPSTREAM_URL" "$CLONE_DIR"
  fi
  git -C "$CLONE_DIR" fetch --quiet --tags origin
  for pair in "$BASE_DIR:$BASE_TAG" "$THEIRS_DIR:$THEIRS_TAG"; do
    dir="${pair%:*}"; tag="${pair##*:}"
    [ -d "$dir" ] || git -C "$CLONE_DIR" worktree add --quiet --detach "$dir" "$tag"
  done
}
