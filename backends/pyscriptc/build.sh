#!/bin/sh
# Build the pyscriptc backend spike binary. Heavy compiles share /tmp/tsc.lock.
set -eu

PYSC=${PYSC:-/opt/mise/installs/python/3.14.9/bin/pyscriptc}
PYREFLY_BIN=${PYREFLY_BIN:-/home/own-agents/src/pyscriptc/.venv/bin/pyrefly}
export PYREFLY_BIN

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
cd "$root"

flock /tmp/tsc.lock "$PYSC" compile backends/pyscriptc/spike.py \
    --emit bin \
    -o backends/pyscriptc/voce_spike \
    --ffi backends/pyscriptc/voce_native.toml \
    --driver clang \
    --linker clang \
    --call main \
    "$@"
