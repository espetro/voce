// NativeShim implementation backed by the voce_native C ABI via scriptc FFI.
//
// FFI fence found during this spike (scriptc 0.4.16, Linux x86-64): in one FFI
// call, only the FIRST 'bytes'/'mutable-bytes' argument may be dereferenced;
// dereferencing a second byte-span argument in the same call segfaults, and a
// trailing scalar after two byte spans also arrives corrupted. Workaround used
// here: pack [window || profile] into ONE concatenated byte span and take a
// pinned pointer per region inside C (validated: cosine == +1.0/-1.0 cases).
//
// Build:
//   clang -c ffi/voce_native_stub.c -O2 -o ffi/voce_native_stub.o
//   ar rcs ffi/libvoce_native_stub.a ffi/voce_native_stub.o
//   flock /tmp/tsc.lock scriptc build src/cli_ffi.ts --ffi ffi/voce_native_stub.json -o voce-scriptc-ffi --no-keep-llvm

declare function voceEmbDim(): number;
declare function voceSimilarity(dim: number, a: Uint8Array, b: Uint8Array): number;

export class NativeFfiShim {
  readonly embeddingDim: number;
  readonly backendName = "voce_native-ffi-stub";
  private packed: Uint8Array;
  private windowBytes: number;

  constructor() {
    this.embeddingDim = voceEmbDim();
    this.windowBytes = this.embeddingDim * 4;
    // Packed buffer: [window (f32 dim) || profile (f32 dim)].
    this.packed = new Uint8Array(this.windowBytes * 2);
  }

  embed(pcm: Float32Array): Float32Array {
    // The stub native shim does not implement embed; the real voce_native will.
    // Filled there; the mock shim covers the embed path in the meantime.
    void pcm;
    throw new Error("embed not implemented in FFI stub shim; use MockShim for embed");
  }

  /**
   * Cosine of window vs enrolled: packs both vectors into the single allowed
   * byte span and lets native code take two pinned pointers.
   */
  similarity(windowEmbedding: Float32Array, enrolled: Float32Array): number {
    const wb = new Uint8Array(windowEmbedding.buffer, windowEmbedding.byteOffset, this.windowBytes);
    const eb = new Uint8Array(enrolled.buffer, enrolled.byteOffset, this.windowBytes);
    this.packed.set(wb, 0);
    this.packed.set(eb, this.windowBytes);
    return voceSimilarity(this.packed.length, this.packed, this.packed);
  }
}
