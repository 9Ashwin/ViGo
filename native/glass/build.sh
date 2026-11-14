#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
mkdir -p native/glass/build
clang++ -std=c++20 -fobjc-arc -O2 -Wall -Wextra -mmacosx-version-min=13.0 -bundle -undefined dynamic_lookup -DNAPI_VERSION=8 -I node_modules/node-api-headers/include native/glass/main.mm -framework AppKit -framework QuartzCore -o native/glass/build/glass.node
