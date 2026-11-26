#!/bin/sh
# Build the pyscriptc backend spike binary. Heavy compiles share /tmp/tsc.lock.
set -eu

PYSC=${PYSC:-/opt/mise/installs/python/3.14.9/bin/pyscriptc}
PYREFLY_BIN=${PYREFLY_BIN:-/home/own-agents/src/pyscriptc/.venv/bin/pyrefly}
export PYREFLY_BIN

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

# Build the native staticlib first. The extern manifest rejects `..` paths, so
# stage the archive beside the manifest before pyscriptc compiles the thunks.
flock /tmp/tsc.lock sh -c '
    cd "$1/native" && cargo build &&
    mkdir -p "$1/backends/pyscriptc/lib" &&
    cp target/debug/libvoce_native.a "$1/backends/pyscriptc/lib/libvoce_native.a"
' sh "$root"

flock /tmp/tsc.lock "$PYSC" compile backends/pyscriptc/spike.py \
    --emit bin \
    -o backends/pyscriptc/voce_spike \
    --ffi backends/pyscriptc/voce_native.toml \
    --driver clang \
    --linker clang \
    --call main \
    "$@"
