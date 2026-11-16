#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
# Avoid rustc strip corrupting proc-macro LINKEDIT alignment on newer dyld (rust-lang/rust#157750).
MACOSX_DEPLOYMENT_TARGET=13.0 CARGO_PROFILE_RELEASE_STRIP=none cargo build --release "$@"
sh native/system-helper/build.sh
sh tools/native/icons.sh
app="release/native/ViGo.app"
version=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys;print(json.load(sys.stdin)["packages"][0]["version"])')
build_version=$(printf '%s' "$version" | python3 -c 'import sys;a,b,c=map(int,sys.stdin.read().split("."));print(a*10000+b*100+c)')
icon_name="ViGo-$(shasum -a 256 build/icon.icns | cut -c1-12).icns"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/ViGo "$app/Contents/MacOS/ViGo"
/usr/bin/strip -x "$app/Contents/MacOS/ViGo"
cp native/system-helper/build/system-helper "$app/Contents/Resources/"
cp assets/mascot-sheet.png build/trayTemplate.png build/trayTemplate@2x.png "$app/Contents/Resources/"
cp assets/companion-browser.html "$app/Contents/Resources/"
rm -f "$app/Contents/Resources"/ViGo-*.icns "$app/Contents/Resources/icon.icns"
cp build/icon.icns "$app/Contents/Resources/$icon_name"
cp LICENSE THIRD_PARTY_LICENSES.md "$app/Contents/Resources/"
rm -f "$app/Contents/Resources/app-icon.png"
python3 tools/native/licenses.py "$app/Contents/Resources/licenses"
cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleIdentifier</key><string>com.vigo.desktop</string>
<key>CFBundleName</key><string>ViGo</string>
<key>CFBundleDevelopmentRegion</key><string>zh-Hans</string>
<key>CFBundleLocalizations</key><array><string>zh-Hans</string></array>
<key>CFBundleDisplayName</key><string>ViGo</string>
<key>CFBundleExecutable</key><string>ViGo</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleURLTypes</key><array><dict>
<key>CFBundleURLName</key><string>com.vigo.desktop.companion</string>
<key>CFBundleURLSchemes</key><array><string>vigo</string></array>
<key>CFBundleTypeRole</key><string>Viewer</string>
</dict></array>
<key>CFBundleIconFile</key><string>$icon_name</string>
<key>CFBundleShortVersionString</key><string>$version</string>
<key>CFBundleVersion</key><string>$build_version</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>LSUIElement</key><true/>
<key>NSHighResolutionCapable</key><true/>
<key>NSPrincipalClass</key><string>NSApplication</string>
<key>NSHumanReadableCopyright</key><string>Copyright © ViGo contributors. MIT license.</string>
</dict></plist>
PLIST
printf 'APPL????' > "$app/Contents/PkgInfo"
identity=${VIGO_SIGN_IDENTITY:-$(security find-identity -v -p codesigning | sed -n 's/.*"\(Apple Development: [^"]*\)".*/\1/p' | head -n 1)}
if test -z "$identity"; then echo 'A stable Apple Development signing identity is required.' >&2; exit 1; fi
codesign --force --sign "$identity" --timestamp=none "$app/Contents/Resources/system-helper"
codesign --force --sign "$identity" --timestamp=none "$app"
codesign --verify --deep --strict "$app"
printf '%s\n' "$app"
