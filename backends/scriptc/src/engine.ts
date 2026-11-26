// Engine: enrollment state, windowing, scoring orchestration, gate decisions,
// and profile serialization. Inference is only touched through NativeShim.

import { type EngineMetrics, type NativeShim, type Profile, type WindowResult,
  WINDOW_SAMPLES, CHUNK_SAMPLES,
} from "./types.ts";

// Deterministic clock: identical values across runs so repeated-run
// determinism is measurable on outputs that embed timings.
export const BenchClock = { nowMs: (): number => 1000 };

/** Modeled FLOP cost of one embed() call (2 flops per multiply-accumulate per band per sample). */
export function opsEstimate(samples: number, bands: number): number {
  return (2 * 8 * samples * bands) / 1e6;
}

const DEFAULT_THRESHOLD = 0.75;
const DEFAULT_VOTE_WINDOW = 3;

export class Engine {
  private shim: NativeShim;
  private profile: Profile | null = null;
  private enrollAccum: Float32Array | null = null;
  private enrollCount = 0;
  private recentSimilarities: number[] = [];
  threshold = DEFAULT_THRESHOLD;
  voteWindow = DEFAULT_VOTE_WINDOW;
  private scoreTimeMs = 0;

  constructor(shim: NativeShim) {
    this.shim = shim;
  }

  resetEnrollment(): void {
    this.profile = null;
    this.enrollAccum = null;
    this.enrollCount = 0;
    this.recentSimilarities = [];
  }

  /** Add PCM to the enrollment accumulation; folds window embeddings. */
  addEnrollmentPcm(pcm: Float32Array): void {
    let off = 0;
    while (off + WINDOW_SAMPLES <= pcm.length) {
      const emb = this.shim.embed(pcm.subarray(off, off + WINDOW_SAMPLES));
      if (this.enrollAccum === null) {
        this.enrollAccum = new Float32Array(this.shim.embeddingDim);
      }
      for (let i = 0; i < this.enrollAccum.length; i++) {
        this.enrollAccum[i] += emb[i];
      }
      this.enrollCount++;
      off += WINDOW_SAMPLES;
    }
    // Tail shorter than a full window is held implicitly by the caller
    // feeding contiguous audio; the spike ignores partial tails.
  }

  /** Finalize enrollment into an averaged, unit-normalized profile. */
  finishEnrollment(): Profile | null {
    if (this.enrollAccum === null || this.enrollCount === 0) return null;
    const emb = new Float32Array(this.enrollAccum.length);
    let norm = 0;
    for (let i = 0; i < emb.length; i++) {
      emb[i] = this.enrollAccum[i] / this.enrollCount;
      norm += emb[i] * emb[i];
    }
    norm = Math.sqrt(norm) || 1;
    for (let i = 0; i < emb.length; i++) emb[i] /= norm;
    this.profile = {
      version: 1,
      embedding_dim: emb.length,
      embedding: Array.from(emb),
      window_count: this.enrollCount,
      threshold: this.threshold,
      vote_window: this.voteWindow,
    };
    return this.profile;
  }

  getProfile(): Profile | null {
    return this.profile;
  }

  loadProfile(p: Profile): void {
    if (p.version !== 1) throw new Error("unsupported profile version " + p.version);
    if (p.embedding.length !== this.shim.embeddingDim) {
      throw new Error("profile dim " + p.embedding.length + " != shim dim " + this.shim.embeddingDim);
    }
    this.profile = p;
    this.threshold = p.threshold;
    this.voteWindow = p.vote_window;
  }

  /** Score one PCM window (up to WINDOW_SAMPLES) against the profile. */
  scoreWindow(pcm: Float32Array): WindowResult {
    if (this.profile === null) throw new Error("not enrolled");
    const t0 = BenchClock.nowMs();
    const emb = this.shim.embed(pcm);
    const enrolled = Float32Array.from(this.profile.embedding);
    const sim = this.shim.similarity(emb, enrolled);
    void opsEstimate(pcm.length, this.shim.embeddingDim);
    this.scoreTimeMs += BenchClock.nowMs() - t0;
    return this.decide(sim, pcm.length);
  }

  /** Apply the vote-window gate to a raw similarity and record it. */
  private decide(similarity: number, samples: number): WindowResult {
    this.recentSimilarities.push(similarity);
    if (this.recentSimilarities.length > this.voteWindow) {
      this.recentSimilarities.shift();
    }
    const passed =
      this.recentSimilarities.length === this.voteWindow &&
      this.recentSimilarities.every((s) => s >= this.threshold);
    return {
      index: this.nextIndex++,
      samples,
      similarity,
      passed,
    };
  }
  private nextIndex = 0;

  /** Batch: window a full PCM buffer on WINDOW_SAMPLES boundaries and score. */
  processPcm(pcm: Float32Array): EngineMetrics {
    this.scoreTimeMs = 0;
    const windows: WindowResult[] = [];
    let off = 0;
    let idx = 0;
    while (off + WINDOW_SAMPLES <= pcm.length) {
      const r = this.scoreWindow(pcm.subarray(off, off + WINDOW_SAMPLES));
      r.index = idx++;
      windows.push(r);
      off += WINDOW_SAMPLES;
    }
    return this.metrics(windows);
  }

  metrics(windows: WindowResult[]): EngineMetrics {
    const n = windows.length || 1;
    return {
      embedding_dim: this.shim.embeddingDim,
      enrolled: this.profile !== null,
      windows,
      score_time_ms: this.scoreTimeMs,
      
      mean_window_time_ms: this.scoreTimeMs / n,
      rss_kib: rssKib(),
    };
  }
}

/** Max RSS of this process in KiB, when the platform exposes it. */
// scriptc has no fs lowering here; read /proc/self/status through a CLI
// helper instead (see cli.ts, which pipes it in). Keep the spike self-contained.
declare function readFileSyncText(path: string): string;

function rssKib(): number | null {
  try {
    const txt = readFileSyncText("/proc/self/status");
    const m = /VmHWM:\s+(\d+)\s+kB/.exec(txt);
    if (m) return parseInt(m[1], 10);
  } catch {}
  return null;
}
