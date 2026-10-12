#!/bin/bash
# Build Dispatch and install it into /Applications, signed.
#
# Why sign: an unsigned build gets a new identity every rebuild, so macOS
# privacy (TCC) silently refuses it access to the iCloud Drive vault — the app
# opens with "No posts found" and logs nothing. Signing with your Apple
# Development certificate gives a stable identity, so the grant sticks.
set -euo pipefail
cd "$(dirname "$0")/.."

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/src-tauri/target}"
nice -n 10 npx tauri build --bundles app
BUILT="$CARGO_TARGET_DIR/release/bundle/macos/Dispatch.app"
APP=/Applications/Dispatch.app

IDENTITY=$(security find-identity -v -p codesigning | awk '/Apple Development/ {print $2; exit}')
[ -n "$IDENTITY" ] || { echo "No Apple Development signing identity found" >&2; exit 1; }

# Keep the installed app's .env (credentials live there, not in the repo).
ENV_BAK=$(mktemp)
chmod 600 "$ENV_BAK"
[ -f "$APP/Contents/Resources/.env" ] && cp "$APP/Contents/Resources/.env" "$ENV_BAK"

osascript -e 'quit app "Dispatch"' 2>/dev/null || true
sleep 2
ditto "$BUILT" "$APP"
[ -s "$ENV_BAK" ] && cp "$ENV_BAK" "$APP/Contents/Resources/.env"
rm -f "$ENV_BAK"

# Sign last: changing any file inside the bundle afterwards breaks the seal.
codesign --force --deep -s "$IDENTITY" "$APP"
codesign --verify --deep --strict "$APP"
echo "Installed $(/usr/libexec/PlistBuddy -c 'Print CFBundleShortVersionString' "$APP/Contents/Info.plist"), signed"
open -a "$APP"
