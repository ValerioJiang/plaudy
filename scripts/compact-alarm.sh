#!/usr/bin/env bash
# scripts/compact-alarm.sh — PreCompact hook.
# Reads the hook JSON on stdin, logs the compaction, raises STOP BUDGET.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 2

INPUT=$(cat 2>/dev/null || true)

TRIGGER=unknown
if command -v jq >/dev/null 2>&1; then
  TRIGGER=$(printf '%s' "$INPUT" | jq -r '.trigger // "unknown"' 2>/dev/null || echo unknown)
fi
[ -n "$TRIGGER" ] || TRIGGER=unknown

mkdir -p .claude
printf '%s\t%s\n' "$(date -u +%FT%TZ)" "$TRIGGER" >> .claude/compact-log.txt

N=$(wc -l < .claude/compact-log.txt | tr -d '[:space:]')

./scripts/alarm.sh STOP BUDGET "compaction #$N ($TRIGGER) — write plan/HANDOFF.md"

exit 2
