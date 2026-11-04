#!/usr/bin/env bash
# scripts/alarm.sh <STOP|DECIDE|FYI> <CODE> <message...>
set -uo pipefail
SEV="${1:?usage: alarm.sh <STOP|DECIDE|FYI> <CODE> <message>}"; shift
CODE="${1:?missing code}"; shift
MSG="$*"
TS=$(date -u +%FT%TZ)
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo -)

mkdir -p plan
[ -f plan/ATTENTION.md ] || printf '# Attention queue\n\nNewest first. Clear a line when you have dealt with it.\n\n' > plan/ATTENTION.md

if [ "$SEV" != "FYI" ]; then
  TMP=$(mktemp)
  { head -4 plan/ATTENTION.md
    printf -- '- [ ] **%s** `%s` %s — %s _(%s)_\n' "$SEV" "$CODE" "$MSG" "$TS" "$BRANCH"
    tail -n +5 plan/ATTENTION.md
  } > "$TMP" && mv "$TMP" plan/ATTENTION.md
fi

case "$SEV" in
  STOP)   printf '\a\033[1;31m\n!! STOP   [%s] %s\033[0m\n' "$CODE" "$MSG" >&2 ;;
  DECIDE) printf '\a\033[1;33m\n?? DECIDE [%s] %s\033[0m\n' "$CODE" "$MSG" >&2 ;;
  *)      printf '\033[2m   fyi [%s] %s\033[0m\n' "$CODE" "$MSG" >&2 ;;
esac

if [ "$SEV" != "FYI" ]; then
  command -v terminal-notifier >/dev/null && terminal-notifier -title "Claude: $SEV" -message "$CODE $MSG" 2>/dev/null
  command -v osascript >/dev/null && osascript -e "display notification \"$CODE $MSG\" with title \"Claude: $SEV\"" 2>/dev/null
  command -v notify-send >/dev/null && notify-send "Claude: $SEV" "$CODE $MSG" 2>/dev/null
fi
exit 0
