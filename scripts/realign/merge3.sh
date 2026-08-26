#!/usr/bin/env bash
# Three-way merge of fork files against upstream, base = $BASE_TAG, theirs = $THEIRS_TAG.
#
#   merge3.sh                 dry run over .audit/census/both-modified.txt, prints conflict counts
#   merge3.sh --apply         same, but writes the merged result (conflict markers included) into handy/
#   merge3.sh path [path...]  restrict to the named files (repo-relative to handy/)
#
# A dry run is the honest measure of the work left. Conflicting files are listed
# with their region count so a reviewer can re-derive the same number.
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
realign_prepare

APPLY=0
declare -a FILES=()
for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=1 ;;
    *) FILES+=("$arg") ;;
  esac
done
if [ "${#FILES[@]}" -eq 0 ]; then
  [ -f "$CENSUS_DIR/both-modified.txt" ] || { echo "run census.sh first" >&2; exit 1; }
  while IFS= read -r f; do FILES+=("$f"); done < "$CENSUS_DIR/both-modified.txt"
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
total=0; clean=0
for f in "${FILES[@]}"; do
  for side in "$BASE_DIR/$f" "$THEIRS_DIR/$f" "$OURS_DIR/$f"; do
    [ -f "$side" ] || { echo "missing: $side" >&2; exit 1; }
  done
  cp -f "$OURS_DIR/$f" "$tmp/merged"
  git merge-file -p --diff3 \
    -L ours -L "upstream $BASE_TAG" -L "upstream $THEIRS_TAG" \
    "$tmp/merged" "$BASE_DIR/$f" "$THEIRS_DIR/$f" > "$tmp/out" 2>/dev/null || true
  n=$(grep -c '^<<<<<<<' "$tmp/out" || true)
  total=$((total + n))
  if [ "$n" -eq 0 ]; then clean=$((clean + 1)); else printf '%4s  %s\n' "$n" "$f"; fi
  [ "$APPLY" -eq 1 ] && cp -f "$tmp/out" "$OURS_DIR/$f"
done
printf '\n%s files, %s auto-clean, %s conflict regions\n' "${#FILES[@]}" "$clean" "$total"
[ "$APPLY" -eq 1 ] && printf 'written into %s\n' "$OURS_DIR"
exit 0
