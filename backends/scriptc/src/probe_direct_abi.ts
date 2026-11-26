// Minimal reproducer for direct FFI against the real Rust cdylib.
// Exercises pointer returns and the one-/two-byte-span fences.
// Build: flock /tmp/tsc.lock scriptc build src/probe_direct_abi.ts \
//   --ffi ffi/direct_abi.json -o probe-direct-abi --no-keep-llvm
declare function voceAbiVersion(): number;
declare function voceEngineCreate(): bigint;
declare function voceEngineDestroy(h: bigint): void;
declare function voceEngineEmbeddingDim(h: bigint, out: Uint8Array): number;
declare function voceEngineEnrollPcm(h: bigint, pcm: Uint8Array): number;
declare function voceEngineDeserializeProfileJson(h: bigint, json: Uint8Array): number;
declare function voceEngineScoreWindow(h: bigint, pcm: Uint8Array, out: Uint8Array): number;

const h = voceEngineCreate();
const dimBytes = new Uint8Array(8);
const dimStatus = voceEngineEmbeddingDim(h, dimBytes);
const dim = dimBytes[0] + dimBytes[1] * 256;
const pcm = new Uint8Array(48000 * 4);
for (let i = 0; i < 48000; i++) {
  const v = Math.sin(2 * Math.PI * 220 * i / 16000) * 0.25;
  const bits = new Uint32Array(1);
  const arr = new Float32Array(bits.buffer);
  arr[0] = v;
  pcm[i * 4] = bits[0] & 255;
  pcm[i * 4 + 1] = (bits[0] >> 8) & 255;
  pcm[i * 4 + 2] = (bits[0] >> 16) & 255;
  pcm[i * 4 + 3] = (bits[0] >> 24) & 255;
}
const enrollStatus = voceEngineEnrollPcm(h, pcm);
// A short zero PCM avoids non-finite reads: n_samples is the byte length (8).
const shortPcm = new Uint8Array(8);
const shortEnrollStatus = voceEngineEnrollPcm(h, shortPcm);
let parts = '{"version":1,"backend":"mock","dim":256,"enrolled":true,"threshold":0.75,"embedding":[';
for (let i = 0; i < 256; i++) parts += i === 0 ? "0.0" : ",0.0";
parts += ']}';
const profile = new TextEncoder().encode(parts);
const deserializeStatus = voceEngineDeserializeProfileJson(h, profile);
const out = new Uint8Array(24);
console.log(JSON.stringify({ abi: voceAbiVersion(), dim_status: dimStatus, dim,
  enroll_status: enrollStatus, short_enroll_status: shortEnrollStatus,
  deserialize_status: deserializeStatus }));
// Two byte spans in one call: known lane B fence. Uncomment to reproduce SIGSEGV.
// const scoreStatus = voceEngineScoreWindow(h, shortPcm, out);
// console.log(JSON.stringify({ score_status: scoreStatus, out: Array.from(out) }));
voceEngineDestroy(h);
