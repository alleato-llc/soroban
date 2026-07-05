#!/usr/bin/env bash
#
# Wrap the portable `soroban-gui` binary in a double-clickable `Soroban.app`
# and pack it into a drag-to-Applications DMG — the macOS leg of the Rust
# release (release-rust.yml). The bundle carries the Swift app's icon
# (AppIcon.icns) so the Dock shows real artwork instead of the generic
# terminal-binary look.
#
# The result is UNSIGNED and un-notarized (the signed, notarized native app is
# the Swift `Soroban.dmg`); first launch still needs a right-click → Open (or
# `xattr -dr com.apple.quarantine Soroban.app`) to clear Gatekeeper. Wrapping a
# GUI binary in a .app is nonetheless a large step up from a bare Mach-O that
# dumps bytes to a terminal when double-clicked.
#
# Usage: make-dmg.sh <binary> <output.dmg> <version>
set -euo pipefail

BIN="${1:?usage: make-dmg.sh <binary> <output.dmg> <version>}"
OUT="${2:?output dmg path required}"
VERSION="${3:?version required}"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ICNS="$HERE/AppIcon.icns"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

APP="$WORK/Soroban.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp "$BIN" "$APP/Contents/MacOS/soroban-gui"
chmod +x "$APP/Contents/MacOS/soroban-gui"
cp "$ICNS" "$APP/Contents/Resources/AppIcon.icns"

printf 'APPL????' > "$APP/Contents/PkgInfo"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>Soroban</string>
	<key>CFBundleDisplayName</key>
	<string>Soroban</string>
	<key>CFBundleExecutable</key>
	<string>soroban-gui</string>
	<key>CFBundleIdentifier</key>
	<string>com.alleato.soroban-gui</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>LSMinimumSystemVersion</key>
	<string>10.15</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
</dict>
</plist>
PLIST

# Stage the .app beside a drag-target /Applications symlink, then compress.
STAGE="$WORK/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"

rm -f "$OUT"
hdiutil create -volname "Soroban" -srcfolder "$STAGE" -ov -format UDZO "$OUT"

echo "Built $OUT (Soroban.app $VERSION, unsigned)"
