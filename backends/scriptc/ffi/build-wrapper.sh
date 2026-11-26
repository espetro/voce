#!/bin/sh
# Build the thin scriptc adaptation wrapper and its Rust staticlib dependency.
# Run from any directory; heavy compiles should be wrapped in `flock /tmp/tsc.lock`.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"
cargo build --manifest-path native/Cargo.toml
clang -std=c11 -O2 -Wall -Wextra -Inative \
  -c native/voce_native_scriptc.c -o backends/scriptc/ffi/voce_native_scriptc.o
ar rcs backends/scriptc/ffi/libvoce_native_scriptc.a \
  backends/scriptc/ffi/voce_native_scriptc.o
echo "built: backends/scriptc/ffi/libvoce_native_scriptc.a"
echo "built: native/target/debug/libvoce_native.a"
