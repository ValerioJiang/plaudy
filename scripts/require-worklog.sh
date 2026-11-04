#!/usr/bin/env bash
# scripts/require-worklog.sh — Stop hook for the implementer agent.
# The worklog must exist and be committed with the newest commit on the branch.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
ID=$(printf '%s' "$BRANCH" | grep -oE '[A-Z]+-[0-9]+' | head -1 || true)

# Not a story branch — nothing to enforce.
[ -n "$ID" ] || exit 0

WORKLOG="plan/stories/$ID.worklog.md"

if [ ! -f "$WORKLOG" ]; then
  printf 'require-worklog: %s is missing. Create it (scripts/new-story.sh %s "<title>" makes one), fill in State / Done / In flight / Tried and rejected / Blocked on / Next action, and commit it with your code.\n' "$WORKLOG" "$ID" >&2
  exit 2
fi

BRANCH_CT=$(git log -1 --format=%ct 2>/dev/null || echo "")
if [ -z "$BRANCH_CT" ]; then
  printf 'require-worklog: no commits on branch %s. Commit your work, including %s, before stopping.\n' "$BRANCH" "$WORKLOG" >&2
  exit 2
fi

WORKLOG_CT=$(git log -1 --format=%ct -- "$WORKLOG" 2>/dev/null || echo "")
if [ -z "$WORKLOG_CT" ]; then
  printf 'require-worklog: %s exists but has never been committed. Commit it (message starting with %s) together with your code.\n' "$WORKLOG" "$ID" >&2
  exit 2
fi

if [ "$WORKLOG_CT" -lt "$BRANCH_CT" ]; then
  printf 'require-worklog: %s is stale — its last commit is older than the newest commit on %s. Update the worklog (In flight, Tried and rejected, Next action) and commit it with the code it describes.\n' "$WORKLOG" "$BRANCH" >&2
  exit 2
fi

exit 0
