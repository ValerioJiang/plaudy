#!/usr/bin/env bash
# Every @rpath dependency of the bundled app must resolve inside the bundle itself.
#
# Why this exists: `bundle.macOS.frameworks` in tauri.conf.json is a hand-maintained list of
# dylib paths, and nothing in the Rust build checks it against what the binary actually links.
# The realign to Handy 0.9.6 changed which onnxruntime dylib `ort` emits — the config kept
# naming a stale versioned file that no longer existed under that name at link time — and the
# result was an app that passed 371 tests and then died at launch with
# `Library not loaded: @rpath/libonnxruntime.dylib`. cargo cannot see this; only the .app can.
#
# Usage: scripts/verify-bundle.sh [path/to/Plaudy.app]
set -euo pipefail

APP="${1:-handy/src-tauri/target/release/bundle/macos/Plaudy.app}"
[ -d "$APP" ] || { echo "no such bundle: $APP" >&2; exit 1; }

BIN="$APP/Contents/MacOS/handy"
FRAMEWORKS="$APP/Contents/Frameworks"

# The search paths dyld will actually try, in the order the binary declares them. @rpath entries
# are read from the load commands rather than assumed, so a change to build.rs's rpath flags
# shows up here instead of silently invalidating the check.
# (`mapfile` is bash 4; macOS ships 3.2, so this reads into an array the portable way.)
RPATHS=()
while read -r rp; do
  RPATHS+=("$rp")
done < <(
  otool -l "$BIN" |
    awk '/cmd LC_RPATH/{f=1} f && /path /{print $2; f=0}' |
    sed -e "s|@executable_path|$APP/Contents/MacOS|" -e "s|@loader_path|$APP/Contents/MacOS|"
)

fail=0
check() { # check <mach-o file>
  local subject="$1" dep base found
  while read -r dep; do
    case "$dep" in @rpath/*) ;; *) continue ;; esac
    base="${dep#@rpath/}"
    found=""
    for dir in "${RPATHS[@]}" "$FRAMEWORKS"; do
      [ -f "$dir/$base" ] && { found="$dir/$base"; break; }
    done
    if [ -n "$found" ]; then
      echo "  ok   $base  <- $(basename "$subject")"
    else
      echo "  MISS $base  <- $(basename "$subject")" >&2
      fail=1
    fi
  done < <(otool -L "$subject" | tail -n +2 | awk '{print $1}')
}

echo "verifying $APP"
check "$BIN"
# Dylibs pull in dylibs: sherpa's c-api needs onnxruntime, and that edge is exactly the one that
# a binary-only check would have declared healthy while the app still failed to launch.
for lib in "$FRAMEWORKS"/*.dylib; do
  [ -e "$lib" ] || continue
  check "$lib"
done

if [ "$fail" -ne 0 ]; then
  echo >&2
  echo "Unresolved @rpath dependency: the bundle will not launch." >&2
  echo "Add the missing dylib to bundle.macOS.frameworks in handy/src-tauri/tauri.conf.json." >&2
  exit 1
fi
echo "all @rpath dependencies resolve inside the bundle"

# Linking is necessary but not sufficient: the app can resolve every dylib and still abort in
# `setup` before it ever draws a window (a Tauri `state()` before its `manage()` does exactly
# that, and the fork's backend-side shortcut init reordered those two). Nothing in `cargo test`
# runs the Tauri setup closure, so the only honest check is to start the thing and see if it is
# still alive afterwards. Set PLAUDY_SKIP_SMOKE=1 to link-check only.
[ -n "${PLAUDY_SKIP_SMOKE:-}" ] && exit 0

echo
echo "smoke: launching for 30s"
log=$(mktemp)
"$BIN" >"$log" 2>&1 &
pid=$!
# Metal shader library init alone can take ~10 s on a cold cache.
for _ in $(seq 30); do
  kill -0 "$pid" 2>/dev/null || break
  sleep 1
done

if kill -0 "$pid" 2>/dev/null; then
  kill "$pid" 2>/dev/null
  wait "$pid" 2>/dev/null || true
  rm -f "$log"
  echo "  ok   still running after 30s"
  exit 0
fi

echo "  DIED during startup:" >&2
# The panic message is on stderr, never in the app log and never in the .ips crash report.
grep -A3 -iE "panicked at|Library not loaded|fatal runtime" "$log" | head -20 >&2 ||
  tail -20 "$log" >&2
rm -f "$log"
exit 1
