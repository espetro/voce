// FFI probe CLI: verifies the NativeShim similarity contract runs against a
// native archive through scriptc --ffi, using the concat-span workaround
// documented in src/native_ffi_shim.ts.
//
// Build:
//   clang -c ffi/voce_native_stub.c -O2 -o ffi/voce_native_stub.o
//   ar rcs ffi/libvoce_native_stub.a ffi/voce_native_stub.o
//   flock /tmp/tsc.lock scriptc build src/cli_ffi.ts --ffi ffi/voce_native_stub.json -o voce-scriptc-ffi --no-keep-llvm

import { readFileSync } from "node:fs";
import { NativeFfiShim } from "./native_ffi_shim.ts";
import { MOCK_EMBEDDING_DIM } from "./types.ts";

function rssKib(): number | null {
  try {
    const txt = readFileSync("/proc/self/status", "utf8");
    const m = /VmHWM:\s+(\d+)\s+kB/.exec(txt);
    if (m) return parseInt(m[1], 10);
  } catch {}
  return null;
}

function mkUnit(v: number): Float32Array {
  const e = new Float32Array(MOCK_EMBEDDING_DIM);
  for (let i = 0; i < e.length; i++) e[i] = v * (i + 1);
  let n = 0;
  for (let i = 0; i < e.length; i++) n += e[i] * e[i];
  n = Math.sqrt(n);
  for (let i = 0; i < e.length; i++) e[i] /= n;
  return e;
}

function main(): void {
  const t0 = Date.now();
  const shim = new NativeFfiShim();
  const self1 = shim.similarity(mkUnit(1), mkUnit(1));
  const self2 = shim.similarity(mkUnit(2), mkUnit(2));
  const neg = shim.similarity(mkUnit(1), mkUnit(-1));
  console.log(JSON.stringify({
    command: "ffi-probe",
    backend: shim.backendName,
    embedding_dim: shim.embeddingDim,
    self_similarity_should_be_1: self1,
    scaled_self_should_be_1: self2,
    opposite_should_be_neg1: neg,
    wall_ms: Date.now() - t0,
    rss_kib: rssKib(),
  }, null, 2));
}
main();
