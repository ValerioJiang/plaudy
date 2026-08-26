#!/usr/bin/env bash
# Classify every file in the fork against upstream $BASE_TAG and $THEIRS_TAG.
# Writes the six lists a reviewer needs to check the realign was complete, into
# .audit/census/, then prints the counts.
#
#   ours-new        files that exist only in the fork
#   ours-deleted    files the fork removed from the base tag
#   ours-modified   files the fork changed (text only; binaries listed separately)
#   up-added        files upstream added between the two tags
#   up-deleted      files upstream removed
#   up-modified     files upstream changed
#   both-modified   the intersection that actually needs merging
source "$(dirname "${BASH_SOURCE[0]}")/env.sh"
realign_prepare

cd "$CLONE_DIR"
git ls-tree -r --name-only "$BASE_TAG" | sort > "$CENSUS_DIR/files-base.txt"
git ls-tree -r --name-only "$THEIRS_TAG" | sort > "$CENSUS_DIR/files-theirs.txt"
git -C "$REPO_ROOT" ls-files handy | sed 's|^handy/||' | sort > "$CENSUS_DIR/files-ours.txt"

comm -13 "$CENSUS_DIR/files-base.txt" "$CENSUS_DIR/files-ours.txt" > "$CENSUS_DIR/ours-new.txt"
comm -23 "$CENSUS_DIR/files-base.txt" "$CENSUS_DIR/files-ours.txt" > "$CENSUS_DIR/ours-deleted.txt"

: > "$CENSUS_DIR/ours-modified.txt"
while IFS= read -r f; do
  cmp -s "$BASE_DIR/$f" "$OURS_DIR/$f" || printf '%s\n' "$f" >> "$CENSUS_DIR/ours-modified.txt"
done < <(comm -12 "$CENSUS_DIR/files-base.txt" "$CENSUS_DIR/files-ours.txt")

git diff --name-status "$BASE_TAG" "$THEIRS_TAG" > "$CENSUS_DIR/up-namestatus.txt"
awk '$1=="A"{print $2}' "$CENSUS_DIR/up-namestatus.txt" | sort > "$CENSUS_DIR/up-added.txt"
awk '$1=="D"{print $2}' "$CENSUS_DIR/up-namestatus.txt" | sort > "$CENSUS_DIR/up-deleted.txt"
awk '$1=="M"{print $2}' "$CENSUS_DIR/up-namestatus.txt" | sort > "$CENSUS_DIR/up-modified.txt"

grep -vE '\.(png|ico|icns|onnx|jpg|woff2?)$' "$CENSUS_DIR/ours-modified.txt" | sort \
  > "$CENSUS_DIR/ours-modified-text.txt"
comm -12 "$CENSUS_DIR/ours-modified-text.txt" "$CENSUS_DIR/up-modified.txt" \
  > "$CENSUS_DIR/both-modified.txt"
comm -13 "$CENSUS_DIR/ours-modified-text.txt" "$CENSUS_DIR/up-modified.txt" \
  > "$CENSUS_DIR/theirs-only-modified.txt"

for n in ours-new ours-deleted ours-modified ours-modified-text \
         up-added up-deleted up-modified both-modified theirs-only-modified; do
  printf '%-22s %5s\n' "$n" "$(wc -l < "$CENSUS_DIR/$n.txt")"
done
