//! Voce native inference shim.
//!
//! Builds as a `cdylib` for FFI consumers and an `rlib` for the Rust CLI probe
//! and tests. The default backend is a deterministic mock; it exercises the
//! full C ABI without ONNX models or downloads.

pub mod abi;
pub mod constants;
pub mod engine;
pub mod metrics;
pub mod real;
pub mod wav;
