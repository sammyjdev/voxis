#!/usr/bin/env bash
# Build GrantMic.app - a tiny Swift helper the tray launches via `open` to raise
# the mic prompt. Shares VOXIS's bundle id so the grant lands on the app.
# macOS only; a no-op elsewhere. Signed here; the outer bundle deep-signs too.
set -euo pipefail
cd "$(dirname "$0")"

[ "$(uname)" = "Darwin" ] || exit 0

APP="resources/GrantMic.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
swiftc -O -o "$APP/Contents/MacOS/GrantMic" grant-mic.swift

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>GrantMic</string>
  <key>CFBundleIdentifier</key><string>dev.sammyj.voxis</string>
  <key>CFBundleExecutable</key><string>GrantMic</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>NSMicrophoneUsageDescription</key><string>VOXIS usa o microfone para transcrever seu ditado por voz.</string>
</dict></plist>
PLIST

codesign --force --sign "VOXIS Dev" "$APP"
echo "built $APP"
