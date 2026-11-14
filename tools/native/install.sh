#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
app="release/native/ViGo.app"
test -x "$app/Contents/MacOS/ViGo"
codesign --verify --deep --strict "$app"
# NSRunningApplication terminates only the installed ViGo, without Apple Events or TCC changes.
swift -e 'import AppKit
for app in NSRunningApplication.runningApplications(withBundleIdentifier: "com.vigo.desktop") where app.bundleURL?.path == "/Applications/ViGo.app" { app.terminate() }'
for attempt in $(seq 1 40); do
  if ! pgrep -f '^/Applications/ViGo.app/Contents/MacOS/ViGo' >/dev/null; then break; fi
  sleep 0.1
done
if pgrep -f '^/Applications/ViGo.app/Contents/MacOS/ViGo' >/dev/null; then
  echo 'ViGo is still open. Close its dialog before installing.' >&2; exit 1
fi
mkdir -p .local/installed-backups
if test -d /Applications/ViGo.app; then
  backup=$(mktemp -d .local/installed-backups/update.XXXXXX)
  mv /Applications/ViGo.app "$backup/ViGo.app.saved"
fi
ditto "$app" /Applications/ViGo.app
codesign --verify --deep --strict /Applications/ViGo.app
open /Applications/ViGo.app
