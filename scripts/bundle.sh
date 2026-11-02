#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
./scripts/icons.sh
cargo build --release --locked --offline
app="$PWD/dist/MacEQ.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/maceq "$app/Contents/MacOS/maceq"
cp bundle/Info.plist "$app/Contents/Info.plist"
cp LICENSE "$app/Contents/Resources/LICENSE"
rm -f "$app/Contents/Resources/MacEQ.icns" "$app/Contents/Resources/MacEQ-PinkBlue.icns"
cp bundle/generated/MacEQ.icns "$app/Contents/Resources/MacEQ-PinkBlue-FullSize.icns"
cp bundle/generated/MenuBarTemplate.png bundle/generated/MenuBarTemplate@2x.png "$app/Contents/Resources/"
cp assets/AppIcon.svg assets/Artwork.svg assets/MenuBarTemplate.svg "$app/Contents/Resources/"
identity="${MACEQ_SIGN_IDENTITY:--}"
if [ "$identity" = "-" ]; then
  codesign --force --sign - --entitlements bundle/MacEQ.entitlements "$app"
else
  codesign --force --options runtime --timestamp --sign "$identity" --entitlements bundle/MacEQ.entitlements "$app"
fi
codesign --verify --strict --verbose=2 "$app"
plutil -lint "$app/Contents/Info.plist"
touch "$app"
echo "Built $app"
