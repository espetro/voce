// NativeShim implementation backed by the voce_native C ABI via scriptc FFI.
// Compiled with: scriptc build src/cli_ffi.ts --ffi ffi/voce_native_stub.json
// Swap the manifest's archive for the real voce_native shim when it lands.

import { MOCK_EMBEDDING_DIM } from "./types.ts";

// Signature-only declarations; bound to C symbols by the FFI manifest.
declare function voceEmbDim(): number;
declare function voceEmbed(pcm: Uint8Array, len: number, out: Uint8Array): number;
declare function voceSimilarity(a: Uint8Array, b: Uint8Array, dim: number): number;

/** f32 view helpers: FFI 'mutable-bytes' operates on byte spans. */
export class NativeFfiShim {
  readonly embeddingDim: number;
  readonly backendName = "voce_native-ffi-stub";

  constructor() {
    this.embeddingDim = voceEmbDim();
  }

  embed(pcm: Float32Array): Float32Array {
    const pcmBytes = new Uint8Array(pcm.buffer, pcm.byteOffset, pcm.byteLength);
    const out = new Float32Array(this.embeddingDim);
    const outBytes = new Uint8Array(out.buffer, out.byteOffset, out.byteLength);
    const rc = voceEmbed(pcmBytes, pcm.length, outBytes);
    if (rc !== 0) throw new Error("voce_embed failed: " + rc);
    return out;
  }

  similarity(windowEmbedding: Float32Array, enrolled: Float32Array): number {
    const a = new Uint8Array(windowEmbedding.buffer, windowEmbedding.byteOffset, windowEmbedding.byteLength);
    const b = new Uint8Array(enrolled.buffer, enrolled.byteOffset, enrolled.byteLength);
    return voceSimilarity(a, b, this.embeddingDim);
  }
}

// Keep the import referenced so tree-shaking never drops the constant used
// by callers that validate dim == MOCK_EMBEDDING_DIM.
void MOCK_EMBEDDING_DIM;
