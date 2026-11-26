// Minimal reproducer: direct scriptc FFI call into the Rust cdylib.
// Build: flock /tmp/tsc.lock scriptc build src/probe_direct_cdylib.ts \
//   --ffi ffi/direct_cdylib.json -o probe-direct-cdylib --no-keep-llvm
declare function voceAbiVersion(): number;
console.log(JSON.stringify({ abi_version: voceAbiVersion() }));
