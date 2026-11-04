#!/usr/bin/env bash
# scripts/status.sh [base-branch]  — reconcile plan/BACKLOG.md against git.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

BASE="${1:-main}"
BACKLOG="plan/BACKLOG.md"

IN_GIT=no
git rev-parse --is-inside-work-tree >/dev/null 2>&1 && IN_GIT=yes

BASE_REF=""
if [ "$IN_GIT" = yes ]; then
  if git rev-parse --verify --quiet "$BASE" >/dev/null 2>&1; then
    BASE_REF="$BASE"
  elif git rev-parse --verify --quiet "origin/$BASE" >/dev/null 2>&1; then
    BASE_REF="origin/$BASE"
  fi
fi

printf '%-12s %-14s %-24s %-8s %-7s %-8s\n' ID CLAIMED BRANCH IN-BASE REVIEW WORKLOG

if [ ! -f "$BACKLOG" ]; then
  printf '(no %s yet)\n' "$BACKLOG"
elif [ "$IN_GIT" != yes ]; then
  printf '(not a git repository — BRANCH and IN-BASE unavailable)\n'
elif [ -z "$BASE_REF" ]; then
  printf '(base branch "%s" not found — IN-BASE unavailable)\n' "$BASE"
fi

ROWS=""
if [ -f "$BACKLOG" ]; then
  ROWS=$(awk -F'|' '
    /^[[:space:]]*\|/ {
      id = $2; st = $4
      gsub(/`/, "", id); gsub(/`/, "", st)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", id)
      gsub(/^[[:space:]]+|[[:space:]]+$/, "", st)
      if (id ~ /^[A-Z]+-[0-9]+$/) print id "\t" st
    }' "$BACKLOG")
fi

if [ -n "$ROWS" ]; then
  while IFS=$'\t' read -r ID CLAIMED; do
    [ -n "$ID" ] || continue
    [ -n "$CLAIMED" ] || CLAIMED="-"

    BRANCH="-"
    INBASE="-"
    if [ "$IN_GIT" = yes ]; then
      B=$(git for-each-ref --format='%(refname:short)' refs/heads 2>/dev/null | grep -F "$ID" | head -1)
      [ -n "$B" ] && BRANCH="$B"
      if [ -n "$BASE_REF" ]; then
        INBASE=$(git log --format=%H --grep="$ID" "$BASE_REF" 2>/dev/null | wc -l | tr -d '[:space:]')
        [ -n "$INBASE" ] || INBASE=0
      fi
    fi

    REVIEW=NO;  [ -f "plan/reviews/$ID.md" ] && REVIEW=YES
    WORKLOG=NO; [ -f "plan/stories/$ID.worklog.md" ] && WORKLOG=YES

    printf '%-12s %-14s %-24s %-8s %-7s %-8s\n' "$ID" "$CLAIMED" "$BRANCH" "$INBASE" "$REVIEW" "$WORKLOG"
  done <<< "$ROWS"
else
  printf '(no stories in the backlog)\n'
fi

printf '\nLook for: claimed=verified with REVIEW=NO, or claimed=merged with IN-BASE=0.\n'
exit 0
