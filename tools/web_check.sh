#!/usr/bin/env bash
# Checks the web build renders, in headless Chrome on the WebGL2 path (what Safari and older
# browsers use). A blank or stuck page is the failure this catches: the loading dialog alone makes a
# tiny screenshot, a rendered map a big one.
#
#   tools/web_check.sh            # check the existing dist/
#   tools/web_check.sh --build    # trunk build --release first
#
# WebGPU doesn't start in headless Chrome on macOS, so the WebGPU path still needs a real browser
# window (see ENGINE.md, "Web build"). Each page gets a fresh browser profile, so nothing is cached.
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--build" ]]; then
  trunk build --release
fi

CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
OUT="${OUT:-$(mktemp -d)}"
PORT="${PORT:-8099}"

(cd dist && python3 -m http.server "$PORT" >/dev/null 2>&1) &
SERVER=$!
trap 'kill $SERVER 2>/dev/null || true' EXIT
sleep 1

status=0
for page in "?webgl" "?webgl&demo"; do
  name="$(echo "$page" | tr -c 'a-z' '_')"
  shot="$OUT/web_check$name.png"
  "$CHROME" --headless=new --use-angle=swiftshader --enable-unsafe-swiftshader \
    --user-data-dir="$(mktemp -d)" --window-size=1280,800 --virtual-time-budget=25000 \
    --screenshot="$shot" "http://localhost:$PORT/$page" >/dev/null 2>&1 || true
  size=$(wc -c <"$shot" 2>/dev/null || echo 0)
  if (( size > 100000 )); then
    echo "ok    $page  ($size bytes)  $shot"
  else
    echo "FAIL  $page  ($size bytes: blank or stuck on loading)  $shot"
    status=1
  fi
done
exit $status
