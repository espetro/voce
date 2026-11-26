// Minimal scriptc consumer of the reviewed native adapter.
//
// It binds only the three adapter symbols. All capacity queries, element-count
// conversions and native output buffers are handled by the C adapter, so this
// file has no native-specific length-pointer logic.

declare function vaCreate(): number;
declare function vaDestroy(session: number): void;
declare function vaCall(session: number, op: number, frame: Uint8Array): number;

const OK = 0;
const BUFFER_TOO_SMALL = -4;
const WINDOW_SAMPLES = 48000;
const HOP_SAMPLES = 24000;
const RESULT_BYTES = 24;
const PREFIX_BYTES = 8;

function requireOk(code: number, label: string): void {
  if (code !== OK) throw new Error(label + " failed: " + code.toString());
}

function i64(value: number): Uint8Array {
  const bytes = new Uint8Array(8);
  const view = new DataView(bytes.buffer);
  view.setInt32(0, value, true);
  view.setInt32(4, Math.floor(value / 4294967296), true);
  return bytes;
}

function readI64(frame: Uint8Array): number {
  const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
  return view.getInt32(0, true);
}

function f32Frame(value: number): Uint8Array {
  const bytes = new Uint8Array(4);
  new DataView(bytes.buffer).setFloat32(0, value, true);
  return bytes;
}

function pcmBytes(samples: number): Uint8Array {
  const bytes = new Uint8Array(samples * 4);
  const view = new DataView(bytes.buffer);
  let i = 0;
  while (i < samples) {
    const value = Math.sin((2 * Math.PI * 220 * i) / 16000) * 0.2;
    view.setFloat32(i * 4, value, true);
    i += 1;
  }
  return bytes;
}

function readResult(frame: Uint8Array, offset: number): number {
  const view = new DataView(frame.buffer, frame.byteOffset + offset, RESULT_BYTES);
  return view.getFloat32(8, true);
}

function run(): void {
  const session = vaCreate();
  if (session < 1) throw new Error("create failed: " + session.toString());

  const path = new TextEncoder().encode("mock://builtin\u0000");
  requireOk(vaCall(session, 1, path), "load_models");
  requireOk(vaCall(session, 2, f32Frame(0.75)), "set_threshold");
  requireOk(vaCall(session, 3, new Uint8Array(0)), "reset");

  const enroll = pcmBytes(WINDOW_SAMPLES);
  requireOk(vaCall(session, 4, enroll), "enroll_pcm");

  const query = i64(0);
  requireOk(vaCall(session, 5, query), "finish_query");
  const dim = readI64(query);
  if (dim !== 256) throw new Error("unexpected embedding dim: " + dim.toString());

  let finishFrame = new Uint8Array(8 + dim * 4);
  finishFrame.set(i64(dim), 0);
  requireOk(vaCall(session, 6, finishFrame), "finish_enrollment");
  if (readI64(finishFrame) !== dim) throw new Error("short embedding");

  const pcm = pcmBytes(WINDOW_SAMPLES);
  const scoreFrame = new Uint8Array(pcm.length + RESULT_BYTES);
  scoreFrame.set(pcm, 0);
  requireOk(vaCall(session, 8, scoreFrame), "score_window");
  const similarity = readResult(scoreFrame, pcm.length);

  const batch = pcmBytes(WINDOW_SAMPLES + HOP_SAMPLES);
  const processQuery = new Uint8Array(8 + batch.length);
  processQuery.set(i64(0), 0);
  processQuery.set(batch, 8);
  requireOk(vaCall(session, 9, processQuery), "process_query");
  const windows = readI64(processQuery);

  const processFrame = new Uint8Array(8 + batch.length + windows * RESULT_BYTES);
  processFrame.set(i64(windows), 0);
  processFrame.set(batch, 8);
  let code = vaCall(session, 10, processFrame);
  if (code === BUFFER_TOO_SMALL) {
    const required = readI64(processFrame);
    const retry = new Uint8Array(8 + batch.length + required * RESULT_BYTES);
    retry.set(i64(required), 0);
    retry.set(batch, 8);
    code = vaCall(session, 10, retry);
    requireOk(code, "process_pcm retry");
  } else {
    requireOk(code, "process_pcm");
  }
  if (readI64(processFrame) !== windows) throw new Error("short result set");

  const jsonQuery = i64(0);
  requireOk(vaCall(session, 11, jsonQuery), "serialize_query");
  const capacity = readI64(jsonQuery);
  if (capacity <= 0) throw new Error("empty profile");
  const jsonFrame = new Uint8Array(8 + capacity);
  jsonFrame.set(i64(capacity), 0);
  requireOk(vaCall(session, 12, jsonFrame), "serialize_profile_json");
  const written = readI64(jsonFrame);
  const json = new TextDecoder().decode(jsonFrame.subarray(8, 8 + written));

  const restored = vaCreate();
  const restoredPath = new TextEncoder().encode("mock://builtin\u0000");
  requireOk(vaCall(restored, 1, restoredPath), "load_models restored");
  requireOk(vaCall(restored, 13, new TextEncoder().encode(json)), "deserialize_profile_json");
  const restoredScoreFrame = new Uint8Array(pcm.length + RESULT_BYTES);
  restoredScoreFrame.set(pcm, 0);
  requireOk(vaCall(restored, 8, restoredScoreFrame), "score_window restored");

  vaDestroy(restored);
  vaDestroy(session);
  console.log(JSON.stringify({
    dim: dim,
    similarity: similarity,
    windows: windows,
    restored_similarity: readResult(restoredScoreFrame, pcm.length),
    json_bytes: written
  }));
}

run();
