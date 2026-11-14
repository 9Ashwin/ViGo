#!/bin/sh
set -eu
cd "$(dirname "$0")"
mkdir -p build
clang++ -std=c++20 -fobjc-arc -O2 -Wall -Wextra -Wno-deprecated-declarations -mmacosx-version-min=13.0 main.mm -framework AppKit -framework ApplicationServices -framework ScreenCaptureKit -framework CoreImage -framework CoreMedia -framework Carbon -o build/system-helper
