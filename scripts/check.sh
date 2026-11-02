#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --locked --offline --all-targets -- -D warnings
cargo test --locked --offline
mkdir -p target/native-tests
xcrun clang -fobjc-arc -mmacosx-version-min=14.4 -Wall -Wextra -Werror \
  -fsanitize=address,undefined tests/native_buffers.m -framework Foundation -framework CoreAudio \
  -o target/native-tests/buffers
target/native-tests/buffers
xcrun clang -fobjc-arc -mmacosx-version-min=14.4 -Wall -Wextra -Werror \
  -fsanitize=address,undefined tests/native_launch.m -framework Foundation -framework CoreServices \
  -o target/native-tests/launch
target/native-tests/launch
