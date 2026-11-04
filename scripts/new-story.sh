#!/usr/bin/env bash
# scripts/new-story.sh <ID> "<title>"  — scaffold a story spec and its worklog.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

ID="${1:?usage: new-story.sh <ID> \"<title>\"}"
TITLE="${2:?usage: new-story.sh <ID> \"<title>\"}"

SPEC="plan/stories/$ID.md"
WORKLOG="plan/stories/$ID.worklog.md"

if [ -e "$SPEC" ]; then
  printf 'new-story: %s already exists — refusing to overwrite it.\n' "$SPEC" >&2
  exit 1
fi

mkdir -p plan/stories

cat > "$SPEC" <<SPEC_EOF
# $ID — $TITLE

## Context
TODO: 2-3 self-contained sentences. An agent with no memory of the planning
conversation must be able to act on this alone.

## Files it may touch
- TODO: path
- TODO: path

Anything not listed here is a spec question, not a judgement call.

## Interfaces it must respect
- TODO: interface ID from docs/adlc/INTERFACES.md

## Acceptance criteria
- [ ] TODO: independently testable criterion, closed with a test name, a
      command, and its actual output.
- [ ] TODO

## Out of scope
- TODO

## Size
TODO: S / M / L

## Routing
Run on: TODO-model at TODO-effort — TODO: one sentence why.
SPEC_EOF

if [ ! -e "$WORKLOG" ]; then
  cat > "$WORKLOG" <<WORKLOG_EOF
# $ID — worklog

Written for someone who has never seen the conversation that produced this
story. Update it whenever you finish a step, hit a dead end, or get blocked,
and commit it with the code.

## State
Created $(date -u +%FT%TZ). Not started.

## Done
- TODO

## In flight
TODO: say explicitly if a file is left half-changed.

## Tried and rejected
TODO: what you ruled out and why — nobody can recover this from the diff.

## Blocked on
TODO

## Next action
TODO: one thing, with a file path.
WORKLOG_EOF
fi

REPO=$(basename "$(pwd)")

printf 'Created %s\n' "$SPEC"
printf 'Created %s\n' "$WORKLOG"
printf '\nNext:\n  git worktree add ../%s-%s -b feature/%s\n' "$REPO" "$ID" "$ID"
exit 0
