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
# Everything is rendered at 2x and displayed in the README at its logical size,
# so the images stay sharp on the Retina and HiDPI screens most people read
# them on. The sizes in SHOTS below are logical points, as they always were;
# SCALE is what turns them into pixels. At 1x the images were soft everywhere
# and the two shown at width=380 were being upscaled outright.
#
# docs/images/icon.png is not from here — that one is the 256x256 out of
# `flodo icon <dir>` (see the README). Note that the iconset it writes is
# uncompressed RGBA, which is fine for iconutil but not for a file in the
# repository, so that one gets recompressed before it lands.
#
# The committed PNGs are then losslessly recompressed, which takes about 30%
# off and changes not a single pixel. Any tool will do — the images were last
# squeezed with Pillow's `save(optimize=True, compress_level=9)`. Skipping it
# only costs bytes, so it is not wired in here as a dependency.
#
#   ./scripts/screenshots.sh              all of them
#   ./scripts/screenshots.sh hero light   just these
#   SCALE=1 ./scripts/screenshots.sh      1x, if you need to compare
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/docs/images"
SCALE="${SCALE:-2}"
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
  "shortcuts|shortcuts|purple|dark|360|760"
  "undo|toast|teal|dark|340|260"
  "empty|empty|blue|dark|340|235"
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
  # The virtual screen has to clear the window at this scale with room to
  # spare: the app asks for clamp_size_to_monitor_size, so a screen smaller
  # than the window silently shrinks the window and the shot comes out wrong
  # rather than failing.
  screen_w=$(( w * SCALE + 200 ))
  screen_h=$(( h * SCALE + 200 ))
  log="$state/flodo.log"
  if xvfb-run -a -s "-screen 0 ${screen_w}x${screen_h}x24" \
    env WINIT_X11_SCALE_FACTOR="$SCALE" \
        LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
        FLODO_STATE_DIR="$state" FLODO_DEMO="$demo" \
        EFRAME_SCREENSHOT_TO="$OUT/$name.png" \
    "$BIN" >"$log" 2>&1
  then
    echo "$OUT/$name.png  ($demo, $accent, $appearance, ${w}x${h} @${SCALE}x)"
  else
    # Without this the failure is invisible: `set -e` kills the script mid-loop
    # and the only clue is that the remaining shots never appear.
    echo "screenshots.sh: $name failed" >&2
    sed 's/^/  /' "$log" >&2
    rm -rf "$state"
    exit 1
  fi
  rm -rf "$state"
done
