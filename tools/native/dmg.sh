#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
test -x release/native/ViGo.app/Contents/MacOS/ViGo
codesign --verify --deep --strict release/native/ViGo.app
mkdir -p .local release
if ! test -x .local/dmg-venv/bin/python; then python3 -m venv .local/dmg-venv; fi
.local/dmg-venv/bin/python -m pip install --disable-pip-version-check -r tools/native/dmg/requirements.txt
swift tools/native/dmg/background.swift release/dmg-background.tiff
version=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' release/native/ViGo.app/Contents/Info.plist)
.local/dmg-venv/bin/dmgbuild -s tools/native/dmg/settings.py "ViGo $version" release/ViGo-macOS-arm64.dmg
hdiutil verify release/ViGo-macOS-arm64.dmg

.local/dmg-venv/bin/python tools/native/verify-dmg.py release/ViGo-macOS-arm64.dmg
