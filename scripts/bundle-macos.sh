#!/usr/bin/env bash
#
# Builds dist/Flodo.app.
#
# Running the bare binary from a terminal works, but it is not a macOS app: no
# icon, and — because a loose executable has no Info.plist — no Retina, so the
# window is rendered at 1x and scaled up. Building the bundle is what makes it
# look right. The release workflow runs this same script.
#
#   ./scripts/bundle-macos.sh                    this machine's architecture
#   ./scripts/bundle-macos.sh --universal        arm64 + x86_64
#   ./scripts/bundle-macos.sh --version 0.2.0    stamp a version into the plist
#
# Set APPLE_SIGNING_IDENTITY to sign with a Developer ID instead of ad-hoc;
# scripts/notarize-macos.sh then takes the result to Apple.
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIST="$ROOT/dist"
APP="$DIST/Flodo.app"
UNIVERSAL=0
VERSION=""

while [ $# -gt 0 ]; do
  case "$1" in
    --universal) UNIVERSAL=1 ;;
    --version) VERSION="${2:?--version needs a value}"; shift ;;
    -h|--help) sed -n '3,12{s/^# \{0,1\}//;p;}' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "bundle-macos.sh: unknown option $1" >&2; exit 1 ;;
  esac
  shift
done

# iconutil, sips, lipo, codesign and plutil are all macOS-only.
if [ "$(uname -s)" != "Darwin" ]; then
  echo "bundle-macos.sh: macOS only (this is $(uname -s))" >&2
  exit 1
fi

cd "$ROOT"
rm -rf "$APP"
mkdir -p "$DIST"

echo "==> Building"
if [ "$UNIVERSAL" = "1" ]; then
  if command -v rustup >/dev/null; then
    rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null
  fi
  cargo build --release --target aarch64-apple-darwin --target x86_64-apple-darwin
  # One binary that runs natively on both Apple Silicon and Intel.
  lipo -create -output "$DIST/flodo" \
    target/aarch64-apple-darwin/release/flodo \
    target/x86_64-apple-darwin/release/flodo
  lipo -info "$DIST/flodo"
else
  cargo build --release
  cp target/release/flodo "$DIST/flodo"
fi
BIN="$DIST/flodo"

# The tag wins over Cargo.toml when the release workflow passes one.
if [ -z "$VERSION" ]; then
  VERSION="$("$BIN" --version | awk '{print $2}')"
fi

echo "==> Icon"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
ICONSET="$("$BIN" icon "$WORK")"
# Every size is rendered at its own resolution; sips only re-encodes, because
# flodo writes its PNG pixels uncompressed and an .icns stores them verbatim.
for png in "$ICONSET"/*.png; do
  sips -s format png "$png" --out "$WORK/compressed.png" >/dev/null
  mv "$WORK/compressed.png" "$png"
done

echo "==> Assembling $APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/flodo"
chmod +x "$APP/Contents/MacOS/flodo"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Flodo.icns"
sed "s/__VERSION__/$VERSION/g" "$ROOT/macos/Info.plist" > "$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null

# A real Developer ID when the environment offers one, ad-hoc otherwise, so a
# contributor without a paid account still gets a working bundle.
#
# The flags differ, not just the identity: notarization requires the hardened
# runtime and a secure timestamp, and neither can be added afterwards — an
# ad-hoc signature is not something notarytool will take. Flodo needs no
# entitlements under the hardened runtime; it JITs nothing and loads no
# unsigned code, and the global hotkey asks for Accessibility at runtime
# rather than through the sandbox.
#
# No --deep: the bundle holds exactly one Mach-O, which signing the bundle
# covers, and Apple has deprecated --deep for signing anything else.
if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  echo "==> Signing as $APPLE_SIGNING_IDENTITY"
  codesign --force --options runtime --timestamp \
    --sign "$APPLE_SIGNING_IDENTITY" "$APP"
else
  echo "==> Signing (ad-hoc — set APPLE_SIGNING_IDENTITY for a real one)"
  codesign --force --sign - "$APP"
fi
codesign --verify --strict --verbose=2 "$APP"

rm -f "$BIN"
echo
echo "Flodo $VERSION → $APP"
echo "Try it with:  open $APP"
