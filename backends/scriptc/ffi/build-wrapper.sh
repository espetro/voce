#!/bin/sh
# Build the thin scriptc adaptation wrapper and its Rust staticlib dependency.
# Run from any directory; heavy compiles should be wrapped in `flock /tmp/tsc.lock`.
# Pass --release to build against native/target/release/libvoce_native.a.
set -eu
mode=debug
if [ "${1:-}" = "--release" ]; then
  mode=release
  shift
fi
if [ "$#" -ne 0 ]; then
  echo "usage: $0 [--release]" >&2
  exit 2
fi
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"
if [ "$mode" = release ]; then
  cargo build --release --manifest-path native/Cargo.toml
  staticlib=native/target/release/libvoce_native.a
else
  cargo build --manifest-path native/Cargo.toml
  staticlib=native/target/debug/libvoce_native.a
fi
clang -std=c11 -O2 -Wall -Wextra -Inative \
  -c native/voce_native_scriptc.c -o backends/scriptc/ffi/voce_native_scriptc.o
ar rcs backends/scriptc/ffi/libvoce_native_scriptc.a \
  backends/scriptc/ffi/voce_native_scriptc.o
echo "built: backends/scriptc/ffi/libvoce_native_scriptc.a"
echo "built: $staticlib"
