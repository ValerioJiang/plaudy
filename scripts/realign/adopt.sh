#!/usr/bin/env bash
# Copy the upstream files that need no merging into the fork: everything upstream
# added between the two tags, plus everything upstream changed that the fork never
# touched. Both lists are filtered through skip.txt.
#
#   adopt.sh          dry run, prints what would be copied and what is skipped
#   adopt.sh --apply  actually copy
#
# Files upstream deleted are reported but never removed automatically, because the
# fork may still depend on one (transcription_mock.rs is exactly that case).
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
realign_prepare

SKIP_FILE="$(dirname "${BASH_SOURCE[0]}")/skip.txt"
APPLY=0
[ "${1:-}" = "--apply" ] && APPLY=1

skipped() {
  local f="$1" pat
  while IFS= read -r pat; do
    case "$pat" in ''|\#*) continue ;; esac
    # shellcheck disable=SC2254
    case "$f" in $pat) return 0 ;; esac
  done < "$SKIP_FILE"
  return 1
}

copied=0; skips=0
for list in up-added theirs-only-modified; do
  echo "== $list"
  while IFS= read -r f; do
    if skipped "$f"; then
      printf '  skip   %s\n' "$f"; skips=$((skips + 1)); continue
    fi
    printf '  adopt  %s\n' "$f"; copied=$((copied + 1))
    if [ "$APPLY" -eq 1 ]; then
      mkdir -p "$OURS_DIR/$(dirname "$f")"
      cp -f "$THEIRS_DIR/$f" "$OURS_DIR/$f"
    fi
  done < "$CENSUS_DIR/$list.txt"
done

echo "== upstream deleted these; decide each one by hand"
sed 's/^/  /' "$CENSUS_DIR/up-deleted.txt"

printf '\n%s adopted, %s skipped\n' "$copied" "$skips"
[ "$APPLY" -eq 1 ] || echo "dry run, nothing written"
