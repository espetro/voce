// Shared types for the scriptc backend spike.
// Mirrors the voce_native semantic contract (16 kHz mono f32 PCM).

export const SAMPLE_RATE = 16000;
export const CHUNK_SAMPLES = 512; // 32 ms tick
export const WINDOW_SAMPLES = 48000; // 3 s inference window
export const MOCK_EMBEDDING_DIM = 64;

export interface WindowResult {
  /** Index of the window in the stream (0-based). */
  index: number;
  /** Number of samples actually scored (<= WINDOW_SAMPLES). */
  samples: number;
  /** Similarity in [-1, 1] (mock shim: cosine over deterministic embeddings). */
  similarity: number;
  /** Gate decision after voting. */
  passed: boolean;
}

export interface EngineMetrics {
  embedding_dim: number;
  enrolled: boolean;
  windows: WindowResult[];
  /** Total wall time scoring windows, ms. */
  score_time_ms: number;
  /** Mean per-window score time, ms. */
  mean_window_time_ms: number;
  /** Max RSS of this process in KiB, when available. */
  rss_kib: number | null;
}

export interface Profile {
  version: 1;
  embedding_dim: number;
  /** Averaged enrollment embedding (unit-normalized). */
  embedding: number[];
  /** Number of windows folded into the profile. */
  window_count: number;
  threshold: number;
  vote_window: number;
}

/** A single window score as returned by VoceWindowResult in voce_native.h. */
export interface VoceNativeWindowScore {
  runtime_ms: number;
  similarity: number;
  speech_probability: number;
  /** VOCE_GATE_PASS (1) or VOCE_GATE_BLOCK (0). */
  decision: number;
}

/**
 * Full stateful ABI seam. It extends NativeShim so existing Engine callers can
 * swap a mock shim for the native adapter without changing their type, while
 * the native CLI drives the richer lifecycle and scoring methods directly.
 */
export interface VoceNativeAdapter extends NativeShim {
  /** ABI embedding_dim query; the property is the value cached at construction. */
  queryEmbeddingDim(): number;
  loadModels(modelsDir: string): void;
  setThreshold(threshold: number): void;
  resetEnrollment(): void;
  enrollPcm(pcm: Float32Array): void;
  finishEnrollment(): Float32Array;
  scoreWindow(pcm: Float32Array): VoceNativeWindowScore;
  processPcm(pcm: Float32Array): VoceNativeWindowScore[];
  serializeProfileJson(): string;
  deserializeProfileJson(json: string): void;
  close(): void;
}

/** Narrow adapter contract mirroring intended voce_native semantics. */
export interface NativeShim {
  /** Compute a deterministic embedding for a PCM window. */
  embed(pcm: Float32Array): Float32Array;
  /** Cosine similarity of a window embedding against an enrolled embedding. */
  similarity(windowEmbedding: Float32Array, enrolled: Float32Array): number;
  readonly embeddingDim: number;
  readonly backendName: string;
}
