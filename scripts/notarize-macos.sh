#!/usr/bin/env bash
#
# Sends dist/Flodo.app to Apple and staples the resulting ticket to it.
#
# This is the step that gets rid of the right-click > Open dance. Signing with
# a Developer ID is not enough on its own — Gatekeeper wants to see that Apple
# has looked at the build, and stapling attaches that verdict to the bundle so
# it holds up on a machine that is offline the first time the app runs.
#
# Staple before you zip: the ticket lives inside the .app, so an archive made
# beforehand carries an unstapled copy.
#
#   ./scripts/notarize-macos.sh                  dist/Flodo.app
#   ./scripts/notarize-macos.sh path/to/My.app
#
# Needs three things in the environment:
#
#   APPLE_ID        the Apple account the Developer ID belongs to
#   APPLE_PASSWORD  an app-specific password for it, *not* the account password
#                   (appleid.apple.com > Sign-In and Security > App-Specific
#                   Passwords)
#   APPLE_TEAM_ID   the ten-character team identifier
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP="${1:-$ROOT/dist/Flodo.app}"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "notarize-macos.sh: macOS only (this is $(uname -s))" >&2
  exit 1
fi
if [ ! -d "$APP" ]; then
  echo "notarize-macos.sh: no bundle at $APP — run bundle-macos.sh first" >&2
  exit 1
fi
for var in APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID; do
  if [ -z "${!var:-}" ]; then
    echo "notarize-macos.sh: $var is not set" >&2
    exit 1
  fi
done

# An ad-hoc signature is rejected on upload with a message that does not say
# so plainly, so check here where the advice can be useful.
if ! codesign --display --verbose=2 "$APP" 2>&1 | grep -q "^Authority=Developer ID"; then
  echo "notarize-macos.sh: $APP is not signed with a Developer ID." >&2
  echo "  Rebuild with APPLE_SIGNING_IDENTITY set." >&2
  exit 1
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

echo "==> Uploading $(basename "$APP")"
# ditto rather than zip: it is what preserves the symlinks and extended
# attributes a bundle depends on.
ditto -c -k --keepParent "$APP" "$WORK/upload.zip"

# --wait blocks until Apple has decided, which is usually a couple of minutes
# and occasionally much longer when their queue is backed up.
OUT="$(xcrun notarytool submit "$WORK/upload.zip" \
  --apple-id "$APPLE_ID" \
  --password "$APPLE_PASSWORD" \
  --team-id "$APPLE_TEAM_ID" \
  --wait 2>&1)" || true
printf '%s\n' "$OUT"

SUBMISSION="$(printf '%s\n' "$OUT" | awk '/^ *id: /{print $2; exit}')"
if ! printf '%s\n' "$OUT" | grep -q 'status: Accepted'; then
  # The summary never says what was actually wrong; the log does.
  if [ -n "$SUBMISSION" ]; then
    echo "==> Rejected. Apple's log:" >&2
    xcrun notarytool log "$SUBMISSION" \
      --apple-id "$APPLE_ID" \
      --password "$APPLE_PASSWORD" \
      --team-id "$APPLE_TEAM_ID" >&2 || true
  fi
  exit 1
fi

echo "==> Stapling"
xcrun stapler staple "$APP"
xcrun stapler validate "$APP"

# The last word belongs to Gatekeeper itself, which is what a user's machine
# will actually consult.
echo "==> Gatekeeper"
spctl --assess --type execute --verbose=4 "$APP"

echo
echo "Notarized and stapled → $APP"
