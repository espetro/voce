#!/bin/sh
# Build and run the minimal scriptc consumer of the reviewed native adapter.
set -eu
root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

flock /tmp/tsc.lock sh -c \
  'cd "$1/native" && CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo build --offline' \
  sh "$root"

mkdir -p backends/adapter-consumers/scriptc/.generated
sed "s|__ROOT__|$root|g" backends/adapter-consumers/scriptc/ffi.template.json \
  > backends/adapter-consumers/scriptc/.generated/ffi.json

out=${1:-/tmp/voce-adapter-scriptc-probe}
flock /tmp/tsc.lock scriptc build backends/adapter-consumers/scriptc/probe.ts \
  --ffi backends/adapter-consumers/scriptc/.generated/ffi.json \
  -o "$out" --no-keep-llvm
"$out"
