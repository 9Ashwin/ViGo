#!/bin/sh
# Builds the macOS input helper (Apple clang, arm64 and x86_64 slices when the host can).
# The helper is a plain C++ program driven over stdin/stdout; see src/hooks_mac.cpp for the event tap.
set -eu
cd "$(dirname "$0")"
mkdir -p build
CXX=${CXX:-clang++}
FLAGS="-std=c++20 -Wall -Wextra -mmacosx-version-min=13.0"
ARCHS=${ARCHS:-$(uname -m)}

for arch in $ARCHS; do
  "$CXX" $FLAGS -arch "$arch" -O2 src/main_posix.cpp src/hooks_mac.cpp src/macinput.cpp src/simulate.cpp \
    src/commands.cpp src/output.cpp src/json.cpp src/decision.cpp \
    -framework ApplicationServices -framework CoreFoundation \
    -o "build/hook-helper-$arch"
done

if [ "$(echo "$ARCHS" | wc -w)" -gt 1 ]; then
  lipo -create build/hook-helper-* -output build/hook-helper
  rm -f build/hook-helper-*
else
  mv "build/hook-helper-$(echo "$ARCHS" | awk '{print $1}')" build/hook-helper
fi

# The decision tests are pure logic; they also cover the launcher's open-on-press variant.
"$CXX" $FLAGS -O0 tests/decision_tests.cpp src/decision.cpp src/commands.cpp src/macinput.cpp src/json.cpp src/output.cpp \
  -o build/decision_tests
echo "Built build/hook-helper and build/decision_tests"
