// Minimal reproducer: two byte-span arguments in one scriptc FFI call into the
// real Rust cdylib. Expected to crash (lane B fence) or return a bad result.
// Build: flock /tmp/tsc.lock scriptc build src/probe_direct_2span.ts \
//   --ffi ffi/direct_abi.json -o probe-direct-2span --no-keep-llvm
declare function voceAbiVersion(): number;
declare function voceEngineEmbeddingDim(h: bigint, out: Uint8Array): number;
declare function voceEngineEnrollPcm(h: bigint, pcm: Uint8Array): number;
declare function voceEngineCreate(): bigint;
declare function voceEngineDestroy(h: bigint): void;
declare function voceEngineDeserializeProfileJson(h: bigint, json: Uint8Array): number;
declare function voceEngineScoreWindow(h: bigint, pcm: Uint8Array, out: Uint8Array): number;

let parts = '{"version":1,"backend":"mock","dim":256,"enrolled":true,"threshold":0.75,"embedding":[';
for (let i = 0; i < 256; i++) parts += i === 0 ? "0.0" : ",0.0";
parts += ']}';
const h = voceEngineCreate();
const json = new TextEncoder().encode(parts);
const restored = voceEngineDeserializeProfileJson(h, json);
console.log("before score:", JSON.stringify({ restored }));
const out = new Uint8Array(24);
const status = voceEngineScoreWindow(h, new Uint8Array(8), out);
console.log("after score:", JSON.stringify({ status, out: Array.from(out) }));
voceEngineDestroy(h);
