// TypeScript adapter over the real stateful voce_native C ABI.
//
// The Rust cdylib links directly through scriptc FFI, but C pointer-to-length
// output parameters cannot be expressed: `mutable-bytes` lowers to
// (ptr, byte_length), so the byte length is dereferenced as an int64_t* and
// the process segfaults. The thin C wrapper in native/voce_native_scriptc.c
// exposes (session, op, frame) instead and implements the native
// capacity/query protocol internally. See
// docs/research/scriptc-native-integration.md for the exact reproducers.
//
// Build (after ffi/build-wrapper.sh):
//   flock /tmp/tsc.lock scriptc build src/cli_native.ts \
//     --ffi ffi/voce_native_scriptc.json -o voce-scriptc-native --no-keep-llvm

import { type NativeShim, type VoceNativeAdapter as VoceNativeAdapterSeam,
  type VoceNativeWindowScore, WINDOW_SAMPLES } from "./types.ts";

declare function vwsCreate(): number;
declare function vwsDestroy(session: number): void;
declare function vwsCall(session: number, op: number, frame: Uint8Array): number;

/** Operation codes from native/voce_native_scriptc.c. */
const OP_LOAD_MODELS = 1;
const OP_SET_THRESHOLD = 2;
const OP_RESET = 3;
const OP_ENROLL = 4;
const OP_FINISH_QUERY = 5;
const OP_FINISH = 6;
const OP_EMBEDDING_DIM = 7;
const OP_SCORE = 8;
const OP_PROCESS_QUERY = 9;
const OP_PROCESS = 10;
const OP_SERIALIZE_QUERY = 11;
const OP_SERIALIZE = 12;
const OP_DESERIALIZE = 13;

const STATUS_BUFFER_TOO_SMALL = -4;
const RESULT_BYTES = 24;

function requireOk(code: number, operation: string): void {
  if (code !== 0) throw new Error(operation + " failed with status " + code);
}

/** Small positive i64 prefix; the upper 32 bits are always zero here. */
function i64Frame(value: number): Uint8Array {
  const frame = new Uint8Array(8);
  const view = new DataView(frame.buffer);
  view.setInt32(0, value, true);
  view.setInt32(4, 0, true);
  return frame;
}

function readI64(frame: Uint8Array): number {
  const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
  return view.getInt32(0, true);
}

function f32Frame(value: number): Uint8Array {
  const frame = new Uint8Array(4);
  new DataView(frame.buffer).setFloat32(0, value, true);
  return frame;
}

function nulTerminated(value: string): Uint8Array {
  const bytes = new TextEncoder().encode(value);
  const frame = new Uint8Array(bytes.length + 1);
  frame.set(bytes, 0);
  frame[bytes.length] = 0;
  return frame;
}

function encodePcm(pcm: Float32Array): Uint8Array {
  const frame = new Uint8Array(pcm.length * 4);
  const view = new DataView(frame.buffer);
  for (let i = 0; i < pcm.length; i++) view.setFloat32(i * 4, pcm[i], true);
  return frame;
}

function decodeEmbedding(frame: Uint8Array, count: number): Float32Array {
  const out = new Float32Array(count);
  const view = new DataView(frame.buffer, frame.byteOffset, frame.byteLength);
  for (let i = 0; i < count; i++) out[i] = view.getFloat32(8 + i * 4, true);
  return out;
}

function decodeResult(frame: Uint8Array, offset: number): VoceNativeWindowScore {
  const view = new DataView(frame.buffer, frame.byteOffset + offset, RESULT_BYTES);
  return {
    runtime_ms: view.getFloat64(0, true),
    similarity: view.getFloat32(8, true),
    speech_probability: view.getFloat32(12, true),
    decision: view.getInt32(16, true),
  };
}

/**
 * One session owns one VoceEngine. `close` destroys it exactly once. Sessions
 * are not thread-safe; use one adapter per thread or serialize access.
 */
export class VoceNativeAdapter implements VoceNativeAdapterSeam, NativeShim {
  readonly embeddingDim: number;
  readonly backendName = "voce_native-staticlib";
  private session: number;

  constructor(modelsDir: string = "mock://builtin") {
    this.session = VoceNativeAdapter.createEngine();
    this.loadModels(modelsDir);
    this.embeddingDim = this.queryEmbeddingDim();
  }

  static createEngine(): number {
    const session = vwsCreate();
    if (session < 0) requireOk(session, "voce_engine_create");
    return session;
  }

  static destroyEngine(session: number): void {
    vwsDestroy(session);
  }

  private static call(session: number, op: number, frame: Uint8Array): number {
    return vwsCall(session, op, frame);
  }

  queryEmbeddingDim(): number {
    const frame = i64Frame(0);
    requireOk(vwsCall(this.session, OP_EMBEDDING_DIM, frame), "embedding_dim query");
    return readI64(frame);
  }

  loadModels(modelsDir: string): void {
    requireOk(vwsCall(this.session, OP_LOAD_MODELS, nulTerminated(modelsDir)),
      "load_models");
  }

  setThreshold(threshold: number): void {
    requireOk(vwsCall(this.session, OP_SET_THRESHOLD, f32Frame(threshold)),
      "set_threshold");
  }

  resetEnrollment(): void {
    requireOk(vwsCall(this.session, OP_RESET, new Uint8Array(0)),
      "reset_enrollment");
  }

  enrollPcm(pcm: Float32Array): void {
    requireOk(vwsCall(this.session, OP_ENROLL, encodePcm(pcm)), "enroll_pcm");
  }

  /** Size query, caller-sized write, then written-count check. */
  finishEnrollment(): Float32Array {
    const query = i64Frame(0);
    requireOk(vwsCall(this.session, OP_FINISH_QUERY, query),
      "finish_enrollment size query");
    let required = readI64(query);
    let frame = new Uint8Array(8 + required * 4);
    frame.set(i64Frame(required), 0);
    let code = vwsCall(this.session, OP_FINISH, frame);
    if (code === STATUS_BUFFER_TOO_SMALL) {
      required = readI64(frame);
      frame = new Uint8Array(8 + required * 4);
      frame.set(i64Frame(required), 0);
      code = vwsCall(this.session, OP_FINISH, frame);
    }
    requireOk(code, "finish_enrollment");
    const written = readI64(frame);
    if (written !== required) {
      throw new Error("finish_enrollment wrote " + written + " of " + required);
    }
    return decodeEmbedding(frame, written);
  }

  scoreWindow(pcm: Float32Array): VoceNativeWindowScore {
    if (pcm.length !== WINDOW_SAMPLES) {
      throw new Error("score_window expects " + WINDOW_SAMPLES + " samples, got " + pcm.length);
    }
    const pcmBytes = encodePcm(pcm);
    const frame = new Uint8Array(pcmBytes.length + RESULT_BYTES);
    frame.set(pcmBytes, 0);
    requireOk(vwsCall(this.session, OP_SCORE, frame), "score_window");
    return decodeResult(frame, pcmBytes.length);
  }

  /** Window count query with a NULL output pointer, then caller-sized results. */
  processPcm(pcm: Float32Array): VoceNativeWindowScore[] {
    const pcmBytes = encodePcm(pcm);
    const query = new Uint8Array(8 + pcmBytes.length);
    query.set(i64Frame(0), 0);
    query.set(pcmBytes, 8);
    requireOk(vwsCall(this.session, OP_PROCESS_QUERY, query), "process_pcm size query");
    let capacity = readI64(query);
    let frame = new Uint8Array(8 + pcmBytes.length + capacity * RESULT_BYTES);
    frame.set(i64Frame(capacity), 0);
    frame.set(pcmBytes, 8);
    let code = vwsCall(this.session, OP_PROCESS, frame);
    if (code === STATUS_BUFFER_TOO_SMALL) {
      capacity = readI64(frame);
      frame = new Uint8Array(8 + pcmBytes.length + capacity * RESULT_BYTES);
      frame.set(i64Frame(capacity), 0);
      frame.set(pcmBytes, 8);
      code = vwsCall(this.session, OP_PROCESS, frame);
    }
    requireOk(code, "process_pcm");
    const written = readI64(frame);
    if (written !== capacity) {
      throw new Error("process_pcm wrote " + written + " of " + capacity + " results");
    }
    const results: VoceNativeWindowScore[] = [];
    for (let i = 0; i < written; i++) {
      results.push(decodeResult(frame, 8 + pcmBytes.length + i * RESULT_BYTES));
    }
    return results;
  }

  /** Required bytes include the trailing NUL; the returned string excludes it. */
  serializeProfileJson(): string {
    const query = i64Frame(0);
    requireOk(vwsCall(this.session, OP_SERIALIZE_QUERY, query),
      "serialize_profile_json size query");
    let required = readI64(query);
    let frame = new Uint8Array(8 + required);
    frame.set(i64Frame(required), 0);
    let code = vwsCall(this.session, OP_SERIALIZE, frame);
    if (code === STATUS_BUFFER_TOO_SMALL) {
      required = readI64(frame);
      frame = new Uint8Array(8 + required);
      frame.set(i64Frame(required), 0);
      code = vwsCall(this.session, OP_SERIALIZE, frame);
    }
    requireOk(code, "serialize_profile_json");
    const written = readI64(frame);
    const bytes = new Uint8Array(written);
    for (let i = 0; i < written; i++) bytes[i] = frame[8 + i];
    return new TextDecoder().decode(bytes);
  }

  deserializeProfileJson(json: string): void {
    requireOk(vwsCall(this.session, OP_DESERIALIZE, new TextEncoder().encode(json)),
      "deserialize_profile_json");
  }

  close(): void {
    VoceNativeAdapter.destroyEngine(this.session);
  }

  /**
   * NativeShim compatibility: the native engine computes embeddings, cosine
   * stays in TypeScript. The native CLI drives scoreWindow/processPcm instead.
   */
  embed(pcm: Float32Array): Float32Array {
    const session = VoceNativeAdapter.createEngine();
    VoceNativeAdapter.call(session, OP_LOAD_MODELS, nulTerminated("mock://builtin"));
    VoceNativeAdapter.call(session, OP_RESET, new Uint8Array(0));
    const enrollCode = VoceNativeAdapter.call(session, OP_ENROLL, encodePcm(pcm));
    requireOk(enrollCode, "embed: enroll_pcm");
    const query = i64Frame(0);
    requireOk(VoceNativeAdapter.call(session, OP_FINISH_QUERY, query),
      "embed: finish_enrollment size query");
    const required = readI64(query);
    const frame = new Uint8Array(8 + required * 4);
    frame.set(i64Frame(required), 0);
    requireOk(VoceNativeAdapter.call(session, OP_FINISH, frame),
      "embed: finish_enrollment");
    const embedding = decodeEmbedding(frame, readI64(frame));
    VoceNativeAdapter.destroyEngine(session);
    return embedding;
  }

  similarity(windowEmbedding: Float32Array, enrolled: Float32Array): number {
    let dot = 0, nw = 0, ne = 0;
    const n = Math.min(windowEmbedding.length, enrolled.length);
    for (let i = 0; i < n; i++) {
      dot += windowEmbedding[i] * enrolled[i];
      nw += windowEmbedding[i] * windowEmbedding[i];
      ne += enrolled[i] * enrolled[i];
    }
    return dot / (Math.sqrt(nw) * Math.sqrt(ne) || 1);
  }
}
