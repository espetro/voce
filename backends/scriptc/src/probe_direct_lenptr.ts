// Minimal reproducer: scriptc FFI cannot express C's int64_t* capacity/length
// output parameters. The 'bytes' lowering passes (ptr, byte_length), so the
// byte length lands in the C argument slot for the length POINTER.
// Build: flock /tmp/tsc.lock scriptc build src/probe_direct_lenptr.ts \
//   --ffi ffi/direct_abi_lenptr.json -o probe-direct-lenptr --no-keep-llvm
declare function voceEngineCreate(): bigint;
declare function voceEngineDestroy(h: bigint): void;
declare function voceEngineEnrollPcm(h: bigint, pcm: Uint8Array): number;
declare function voceEngineFinishEnrollment(h: bigint, out: Uint8Array, len: Uint8Array): number;

const h = voceEngineCreate();
console.log("enroll:", voceEngineEnrollPcm(h, new Uint8Array(8)));
console.log("before finish_enrollment");
const embedding = new Uint8Array(256 * 4);
const lenSlot = new Uint8Array(8);
const status = voceEngineFinishEnrollment(h, embedding, lenSlot);
console.log("after finish_enrollment:", JSON.stringify({ status }));
voceEngineDestroy(h);
