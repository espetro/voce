// Smoke test: thin wrapper + Rust staticlib through scriptc FFI.
// Build: flock /tmp/tsc.lock scriptc build src/probe_wrapper.ts \
//   --ffi ffi/voce_native_scriptc.json -o probe-wrapper --no-keep-llvm
declare function vwsCreate(): number;
declare function vwsDestroy(session: number): void;
declare function vwsCall(session: number, op: number, frame: Uint8Array): number;

// All counts here are small positive integers; the upper i32 half is zero.
function i64(v: number): Uint8Array {
  const out = new Uint8Array(8);
  const view = new DataView(out.buffer);
  view.setInt32(0, v, true);
  view.setInt32(4, 0, true);
  return out;
}
function readI64(bytes: Uint8Array): number {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return v.getInt32(0, true);
}

const session = vwsCreate();
const dimQuery = i64(0);
const dimStatus = vwsCall(session, 7, dimQuery);
const dim = readI64(dimQuery);
const path = new TextEncoder().encode("mock://builtin\u0000");
const loadStatus = vwsCall(session, 1, path);
const resetStatus = vwsCall(session, 3, new Uint8Array(0));
const pcm = new Uint8Array(48000 * 4);
for (let i = 0; i < 48000; i++) {
  const f = new Float32Array(1);
  f[0] = Math.sin(2 * Math.PI * 220 * i / 16000) * 0.25;
  const b = new Uint8Array(f.buffer);
  pcm.set(b, i * 4);
}
const enrollStatus = vwsCall(session, 4, pcm);
const finishQuery = i64(0);
const finishQueryStatus = vwsCall(session, 5, finishQuery);
const required = readI64(finishQuery);
const finishFrame = new Uint8Array(8 + required * 4);
finishFrame.set(i64(required), 0);
const finishStatus = vwsCall(session, 6, finishFrame);
const scoreFrame = new Uint8Array(pcm.length + 24);
scoreFrame.set(pcm, 0);
const scoreStatus = vwsCall(session, 8, scoreFrame);
const result = new DataView(scoreFrame.buffer, scoreFrame.byteOffset + pcm.length, 24);
const similarity = result.getFloat32(8, true);
const decision = result.getInt32(16, true);
console.log(JSON.stringify({ session, dimStatus, dim, loadStatus, resetStatus,
  enrollStatus, finishQueryStatus, required, finishStatus, scoreStatus,
  similarity, decision }));
vwsDestroy(session);
