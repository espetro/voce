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

/** Narrow adapter contract mirroring intended voce_native semantics. */
export interface NativeShim {
  /** Compute a deterministic embedding for a PCM window. */
  embed(pcm: Float32Array): Float32Array;
  /** Cosine similarity of a window embedding against an enrolled embedding. */
  similarity(windowEmbedding: Float32Array, enrolled: Float32Array): number;
  readonly embeddingDim: number;
  readonly backendName: string;
}
