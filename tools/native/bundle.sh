#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
MACOSX_DEPLOYMENT_TARGET=13.0 cargo build --release "$@"
sh native/system-helper/build.sh
app="release/native/ViGo.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/ViGo "$app/Contents/MacOS/ViGo"
cp native/system-helper/build/system-helper "$app/Contents/Resources/"
cp assets/mascot-sheet.png build/icon.icns build/trayTemplate.png build/trayTemplate@2x.png "$app/Contents/Resources/"
cp LICENSE THIRD_PARTY_LICENSES.md "$app/Contents/Resources/"
rm -f "$app/Contents/Resources/app-icon.png"
python3 tools/native/licenses.py "$app/Contents/Resources/licenses"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.vigo.desktop</string>
<key>CFBundleName</key><string>ViGo</string>
<key>CFBundleDevelopmentRegion</key><string>zh-Hans</string>
<key>CFBundleLocalizations</key><array><string>zh-Hans</string></array>
<key>CFBundleDisplayName</key><string>ViGo</string>
<key>CFBundleExecutable</key><string>ViGo</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleIconFile</key><string>icon.icns</string>
<key>CFBundleShortVersionString</key><string>0.3.0</string>
<key>CFBundleVersion</key><string>300</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSHumanReadableCopyright</key><string>Copyright © ViGo contributors. MIT license.</string>
</dict></plist>
PLIST
identity=${VIGO_SIGN_IDENTITY:-$(security find-identity -v -p codesigning | sed -n 's/.*"\(Apple Development: [^"]*\)".*/\1/p' | head -n 1)}
if test -z "$identity"; then echo 'A stable Apple Development signing identity is required.' >&2; exit 1; fi
codesign --force --sign "$identity" --timestamp=none "$app/Contents/Resources/system-helper"
codesign --force --sign "$identity" --timestamp=none "$app"
codesign --verify --deep --strict "$app"
printf '%s\n' "$app"
