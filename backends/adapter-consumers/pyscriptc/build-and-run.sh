#!/bin/sh
# Build and run the minimal pyscriptc consumer of the reviewed native adapter.
set -eu
PYSC=${PYSC:-/opt/mise/installs/python/3.14.9/bin/pyscriptc}
PYREFLY_BIN=${PYREFLY_BIN:-/home/own-agents/src/pyscriptc/.venv/bin/pyrefly}
export PYREFLY_BIN

root=$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)
cd "$root"

flock /tmp/tsc.lock sh -c '
  cd "$1/native" &&
  CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 cargo build --offline &&
  mkdir -p "$1/backends/adapter-consumers/pyscriptc/lib" &&
  cp target/debug/libvoce_native.a "$1/backends/adapter-consumers/pyscriptc/lib/libvoce_native.a" &&
  cp target/debug/libvoce_adapter.a "$1/backends/adapter-consumers/pyscriptc/lib/libvoce_adapter.a"
' sh "$root"

out=${1:-/tmp/voce-adapter-pyscriptc-probe}
flock /tmp/tsc.lock "$PYSC" compile backends/adapter-consumers/pyscriptc/probe.py \
  --emit bin -o "$out" \
  --ffi backends/adapter-consumers/pyscriptc/voce_adapter.toml \
  --driver clang --linker clang --call run
"$out" "$@"
