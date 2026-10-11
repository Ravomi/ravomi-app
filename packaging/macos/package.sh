#!/bin/bash
set -euo pipefail

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$project_dir"
if [[ "$(uname -s)" != Darwin || "$(uname -m)" != arm64 ]]; then
    echo 'Run this packager on an Apple Silicon Mac.' >&2
    exit 1
fi
export MACOSX_DEPLOYMENT_TARGET=13.0
cargo build --release --locked --target aarch64-apple-darwin
version="$(cargo read-manifest | plutil -extract version raw -o - -)"
output="$project_dir/target/macos-package"
mkdir -p "$output"
stage="$(mktemp -d "$output/stage.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
bundle="$stage/Ravomi.app"
mkdir -p "$bundle/Contents/MacOS" "$bundle/Contents/Resources"
cp target/aarch64-apple-darwin/release/project-graph-bevy-canvas "$bundle/Contents/MacOS/Ravomi"
cp packaging/macos/Info.plist "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$bundle/Contents/Info.plist"
cp LICENSE CREDITS.md "$bundle/Contents/Resources/"
mkdir -p "$bundle/Contents/Resources/Licenses"
cp assets/fonts/OFL-*.txt "$bundle/Contents/Resources/Licenses/"
iconset="$stage/Ravomi.iconset"
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" assets/branding/app-icons/1024x1024.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    doubled=$((size * 2))
    sips -z "$doubled" "$doubled" assets/branding/app-icons/1024x1024.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$bundle/Contents/Resources/Ravomi.icns"
rm -rf "$iconset"
codesign --force --sign - "$bundle"
codesign --verify --deep --strict "$bundle"
plutil -lint "$bundle/Contents/Info.plist"
ln -s /Applications "$stage/Applications"
cp packaging/macos/安装说明.txt "$stage/安装说明.txt"
dmg="$output/Ravomi-$version-apple-silicon.dmg"
hdiutil create -volname "Ravomi $version" -srcfolder "$stage" -ov -format UDZO "$dmg"
hdiutil verify "$dmg"
shasum -a 256 "$dmg" > "$dmg.sha256"
echo "$dmg"
