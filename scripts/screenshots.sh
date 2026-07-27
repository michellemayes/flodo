#!/usr/bin/env bash
#
# Regenerates every screenshot in docs/images.
#
# eframe has a built-in hook that renders a couple of frames, writes a PNG and
# exits, so this needs nothing beyond Xvfb and a software GL stack. FLODO_DEMO
# seeds a scenario in memory and FLODO_STATE_DIR keeps it away from your real
# list; the settings file written per shot is what fixes the window size,
# accent and light/dark, so the images stay reproducible instead of depending
# on whoever ran it last.
#
# docs/images/icon.png is not from here — that one is `flodo icon` (see the
# README).
#
#   ./scripts/screenshots.sh              all of them
#   ./scripts/screenshots.sh hero light   just these
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/docs/images"
cd "$ROOT"

for tool in xvfb-run cargo; do
  command -v "$tool" >/dev/null || { echo "screenshots.sh: need $tool" >&2; exit 1; }
done

cargo build --features screenshot
BIN="$ROOT/target/debug/flodo"

# name | demo | accent | appearance | width | height
SHOTS=(
  "hero|hero|pink|dark|340|275"
  "light|hero|blue|light|340|275"
  "editing|editing|pink|dark|330|200"
  "rendered|rendered|pink|dark|330|200"
  "markdown|showcase|teal|dark|360|455"
  "settings|settings|purple|dark|340|460"
  "accent-pink|hero|pink|dark|300|260"
  "accent-green|hero|green|light|300|260"
  "accent-amber|hero|amber|dark|300|260"
  "accent-purple|hero|purple|light|300|260"
)

wanted=("$@")
matches() {
  [ ${#wanted[@]} -eq 0 ] && return 0
  local want
  for want in "${wanted[@]}"; do [ "$want" = "$1" ] && return 0; done
  return 1
}

for shot in "${SHOTS[@]}"; do
  IFS='|' read -r name demo accent appearance w h <<< "$shot"
  matches "$name" || continue

  state="$(mktemp -d)"
  cat > "$state/settings.json" <<JSON
{"version":1,"accent":"$accent","appearance":"$appearance","window":{"w":$w.0,"h":$h.0}}
JSON
  xvfb-run -a -s "-screen 0 700x900x24" \
    env LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
        FLODO_STATE_DIR="$state" FLODO_DEMO="$demo" \
        EFRAME_SCREENSHOT_TO="$OUT/$name.png" \
    "$BIN" >/dev/null 2>&1
  rm -rf "$state"
  echo "$OUT/$name.png  ($demo, $accent, $appearance, ${w}x${h})"
done
