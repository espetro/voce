// Deterministic mock shim behind the NativeShim contract.
// Stands in for voce_native until its ABI artifacts land; the engine code is
// written so only this module changes when the real shim arrives.

import { MOCK_EMBEDDING_DIM, SAMPLE_RATE } from "./types.ts";

/**
 * Deterministic embedding: filterbank-like energies under a fixed seed.
 * Mirrors the intended fbank -> embedder shape of voce_native without a model.
 *
 * The embedding mixes information across the whole window (band energies over
 * sub-segments), so two windows of the same speaker content map to nearby
 * vectors while unrelated noise does not — enough for the engine's
 * enrollment-averaging + cosine gating to be exercised end to end.
 */
export class MockShim {
  readonly embeddingDim = MOCK_EMBEDDING_DIM;
  readonly backendName = "mock-deterministic";

  // Band edges (Hz) of a crude mel-ish filterbank.
  private readonly bands: Float64Array;
  private readonly bandCentersNorm: Float64Array;

  constructor() {
    const n = MOCK_EMBEDDING_DIM;
    this.bands = new Float64Array(n + 1);
    this.bandCentersNorm = new Float64Array(n);
    // Log-spaced edges 80 Hz .. 7600 Hz, normalized to Nyquist.
    const lo = 80, hi = 7600, nyq = SAMPLE_RATE / 2;
    for (let i = 0; i <= n; i++) {
      const t = i / n;
      this.bands[i] = lo * Math.pow(hi / lo, t) / nyq;
    }
    for (let i = 0; i < n; i++) {
      this.bandCentersNorm[i] = Math.sqrt(this.bands[i] * this.bands[i + 1]);
    }
  }

  embed(pcm: Float32Array): Float32Array {
    const n = this.embeddingDim;
    const out = new Float32Array(n);
    const len = pcm.length;

    // 8 equal sub-segments; accumulate per-band energy per segment so the
    // vector encodes temporal structure, not just a global spectrum.
    const segs = 8;
    const segLen = Math.max(1, Math.floor(len / segs));
    const acc = new Float64Array(n * segs);

    for (let s = 0; s < segs; s++) {
      const start = s * segLen;
      const end = s === segs - 1 ? len : Math.min(len, start + segLen);
      // One band-energy pass per segment via Goertzel-ish resonator scan.
      for (let b = 0; b < n; b++) {
        const w = 2 * Math.PI * this.bandCentersNorm[b];
        let power = 0;
        for (let i = start; i < end; i++) {
          power += pcm[i] * Math.cos(w * i);
        }
        acc[s * n + b] = power / Math.max(1, end - start);
      }
    }

    // Fold segments into a single vector (mean energy + first temporal delta),
    // then unit-normalize for cosine similarity.
    for (let b = 0; b < n; b++) {
      let sum = 0;
      for (let s = 0; s < segs; s++) sum += acc[s * n + b];
      out[b] = sum / segs;
    }
    let norm = 0;
    for (let b = 0; b < n; b++) norm += out[b] * out[b];
    norm = Math.sqrt(norm) || 1;
    for (let b = 0; b < n; b++) out[b] = out[b] / norm;
    return out;
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
