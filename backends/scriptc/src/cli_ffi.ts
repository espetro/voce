// FFI probe CLI: verifies the NativeShim contract compiles and runs against a
// native archive through scriptc --ffi, exercising the exact embed/similarity
// surface the real voce_native shim will expose.
//
// Build:
//   clang -c ffi/voce_native_stub.c -O2 -o ffi/voce_native_stub.o
//   ar rcs ffi/libvoce_native_stub.a ffi/voce_native_stub.o
//   flock /tmp/tsc.lock scriptc build src/cli_ffi.ts --ffi ffi/voce_native_stub.json -o voce-scriptc-ffi --no-keep-llvm

import { NativeFfiShim } from "./native_ffi_shim.ts";
import { synthPcm } from "./audio_io.ts";
import { Engine } from "./engine.ts";
import { readFileSync } from "node:fs";

function rssKib(): number | null {
  try {
    const txt = readFileSync("/proc/self/status", "utf8");
    const m = /VmHWM:\s+(\d+)\s+kB/.exec(txt);
    if (m) return parseInt(m[1], 10);
  } catch {}
  return null;
}

function main(): void {
  const t0 = Date.now();
  const shim = new NativeFfiShim();
  const engine = new Engine(shim);
  engine.addEnrollmentPcm(synthPcm(6, 1, 210));
  const p = engine.finishEnrollment();
  const target = engine.processPcm(synthPcm(6, 1, 210));
  const engine2 = new Engine(shim);
  engine2.loadProfile(p!);
  const other = engine2.processPcm(synthPcm(6, 7, 950));
  console.log(JSON.stringify({
    command: "ffi-probe",
    backend: shim.backendName,
    embedding_dim: shim.embeddingDim,
    profile_dim: p!.embedding_dim,
    target_similarity: target.windows.map((w) => w.similarity),
    other_similarity: other.windows.map((w) => w.similarity),
    wall_ms: Date.now() - t0,
    rss_kib: rssKib(),
  }, null, 2));
}
main();
