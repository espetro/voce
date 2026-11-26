//! Small metric helpers used by the CLI probe.
//!
//! Runtime is measured per window inside the ABI result. RSS is read from
//! Linux `/proc`; non-Linux hosts report null RSS fields rather than failing.

use crate::abi::VoceWindowResult;

/// Current resident set size in bytes (Linux only).
pub fn current_rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages = statm.split_whitespace().nth(1)?.parse::<u64>().ok()?;
    // Linux page size on the spike hosts is 4096 bytes. Documented as an
    // approximation; the probe reports bytes, not an exact accounting.
    Some(resident_pages * 4096)
}

/// Peak resident set size in bytes from `/proc/self/status` (Linux only).
pub fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(value) = line.strip_prefix("VmHWM:") {
            let kib = value.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kib * 1024);
        }
    }
    None
}

/// FNV-1a digest over deterministic window outputs (timings are excluded).
pub fn digest_results(results: &[VoceWindowResult]) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for result in results {
        for word in [
            result.similarity.to_bits() as u64,
            result.speech_probability.to_bits() as u64,
            result.decision as u64,
        ] {
            hash ^= word;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}
