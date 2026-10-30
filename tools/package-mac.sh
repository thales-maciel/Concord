#!/bin/bash
# Package an ad-hoc signed Apple Silicon app in dist/concord-<version>-mac-arm64.zip.
# Requires macOS, aarch64 Rust and Xcode Command Line Tools.
# Usage: tools/package-mac.sh [-j N] [--skip-build]
# Honors CARGO_TARGET_DIR; defaults to 3 build jobs for 8 GB machines.
set -euo pipefail

JOBS=3
BUILD=1
while [ $# -gt 0 ]; do
  case "$1" in
    -j) JOBS="$2"; shift 2 ;;
    --skip-build) BUILD=0; shift ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

[ "$(uname -s)" = "Darwin" ] || { echo "run this on a Mac" >&2; exit 1; }
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

VERSION="$(awk '/^\[workspace.package\]/{f=1; next} /^\[/{f=0} f && /^version *=/{gsub(/[" \r]/, "", $0); split($0, a, "="); print a[2]; exit}' Cargo.toml)"
[ -n "$VERSION" ] || { echo "no version in Cargo.toml" >&2; exit 1; }
echo "concord $VERSION for macOS (arm64)"

if [ "$BUILD" = 1 ]; then
  # Redact builder paths. RUSTFLAGS overrides .cargo/config.toml, so repeat -ObjC here.
  RUSTFLAGS="-C link-args=-ObjC --remap-path-prefix=$HOME=~ --remap-path-prefix=$ROOT=concord" \
    CARGO_INCREMENTAL=0 cargo build --locked --release -p voxhub-app -j "$JOBS"
fi
BIN="$TARGET_DIR/release/concord"
[ -x "$BIN" ] || { echo "no binary at $BIN" >&2; exit 1; }
[ "$(lipo -archs "$BIN" 2>/dev/null || echo arm64)" = "arm64" ] || echo "warning: $BIN is not arm64-only" >&2

OUT="$ROOT/dist"
STAGE="$OUT/concord-$VERSION-mac"
APP="$STAGE/concord.app"
rm -rf "$STAGE"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/concord"
cp "$ROOT/app/assets/logo/concord.icns" "$APP/Contents/Resources/concord.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleDisplayName</key>
	<string>concord</string>
	<key>CFBundleExecutable</key>
	<string>concord</string>
	<key>CFBundleIconFile</key>
	<string>concord</string>
	<key>CFBundleIdentifier</key>
	<string>app.concord.desktop</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>concord</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>$VERSION</string>
	<key>CFBundleVersion</key>
	<string>$VERSION</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.social-networking</string>
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSSupportsAutomaticGraphicsSwitching</key>
	<true/>
	<key>NSMicrophoneUsageDescription</key>
	<string>concord uses the microphone for voice rooms and calls.</string>
	<key>NSCameraUsageDescription</key>
	<string>concord uses the camera when you turn it on in a room.</string>
	<key>NSLocalNetworkUsageDescription</key>
	<string>concord connects to your group's server, which may be on your home network.</string>
	<key>NSScreenCaptureUsageDescription</key>
	<string>concord captures the screen or window you choose to share with friends.</string>
</dict>
</plist>
PLIST
plutil -lint "$APP/Contents/Info.plist" > /dev/null
printf 'APPL????' > "$APP/Contents/PkgInfo"

sed "s/@VERSION@/$VERSION/g" "$ROOT/tools/README-MAC.txt" > "$STAGE/README-MAC.txt"
# Preserve the filename used by existing download instructions.
cp "$STAGE/README-MAC.txt" "$STAGE/LEIA-ME-MAC.txt"

codesign --force --deep -s - "$APP"
codesign --verify --deep --strict "$APP"

ZIP="$OUT/concord-$VERSION-mac-arm64.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$STAGE" "$ZIP"
echo "ok: $ZIP ($(du -h "$ZIP" | cut -f1))"
