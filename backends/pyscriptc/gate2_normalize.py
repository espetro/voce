#!/usr/bin/env python3
"""Normalize one native-probe/pyscriptc pair for Gate 2 comparison.

Both probes emit raw per-window ABI decisions and do not implement the app's
vote gate. This helper applies the canonical `SlidingVoteGate` semantics from
`src/filter/gate.rs` to both sides: fail-open, mute only when all similarities
in the last `vote_window` windows are below threshold, with vote history reset
at the start of each input file. Repeat 1 and repeat 2 are independent
observations; history never carries from one repeat into the next.

The recomputed Rust FNV-1a digest is derived from pyscriptc window fields so
parity can be checked without changing either CLI.
"""

import argparse
import json
import struct
from pathlib import Path


def fnv_digest(windows: list[dict]) -> str:
    h = 0xCBF29CE484222325
    for window in windows:
        for key in ("similarity", "speech_probability", "decision"):
            value = window[key]
            word = (
                struct.unpack("<I", struct.pack("<f", float(value)))[0]
                if key != "decision"
                else int(value)
            )
            h ^= word
            h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def vote(windows: list[dict], threshold: float = 0.75, size: int = 3) -> list[int]:
    """Canonical app vote gate, evaluated independently for this file/repeat.

    `src/filter/gate.rs::SlidingVoteGate` starts in pass state and mutes only
    when the history is full and every recent frame failed. Frame pass is
    decided from similarity, not from the ABI raw decision: raw decisions also
    pass silence windows, while the app gate compares similarity directly.
    """
    recent: list[bool] = []
    result: list[int] = []
    for window in windows:
        recent.append(float(window["similarity"]) >= threshold)
        recent = recent[-size:]
        muted = len(recent) == size and not any(recent)
        result.append(0 if muted else 1)
    return result


def native_windows(report: dict, path: str, repeat: int = 0) -> list[dict]:
    runs = report["runs"]
    if repeat < 0 or repeat >= len(runs):
        raise SystemExit(f"no native repeat {repeat} in report")
    for item in runs[repeat]["inputs"]:
        if Path(item["path"]).name == Path(path).name:
            return item["windows"]
    raise SystemExit(f"no native input matching {path} in repeat {repeat}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", required=True, help="voce-native-probe JSON report")
    parser.add_argument("--pyscriptc", required=True, help="voce_spike JSON line")
    parser.add_argument("--input", required=True, help="WAV basename to compare")
    parser.add_argument("--repeat", type=int, default=0, help="native run index (0-based)")
    parser.add_argument("--threshold", type=float, default=0.75)
    parser.add_argument("--vote-window", type=int, default=3)
    args = parser.parse_args()

    native = native_windows(
        json.loads(Path(args.native).read_text()), args.input, args.repeat
    )
    ps_report = json.loads(Path(args.pyscriptc).read_text())
    ps = ps_report["windows"]
    native_vote = vote(native, args.threshold, args.vote_window)
    ps_vote = vote(ps, args.threshold, args.vote_window)

    raw_mismatches = sum(
        1 for a, b in zip(native, ps) if a["decision"] != b["decision"]
    )
    vote_mismatches = sum(1 for a, b in zip(native_vote, ps_vote) if a != b)
    similarity_mismatches = sum(
        1
        for a, b in zip(native, ps)
        if struct.pack("<f", a["similarity"]) != struct.pack("<f", b["similarity"])
    )
    print(
        json.dumps(
            {
                "input": args.input,
                "repeat": args.repeat,
                "native_window_count": len(native),
                "pyscriptc_window_count": len(ps),
                "native_raw_pass": sum(int(w["decision"]) for w in native),
                "pyscriptc_raw_pass": sum(int(w["decision"]) for w in ps),
                "native_vote_pass": sum(native_vote),
                "pyscriptc_vote_pass": sum(ps_vote),
                "raw_decision_mismatches": raw_mismatches,
                "vote_gated_decision_mismatches": vote_mismatches,
                "similarity_mismatches": similarity_mismatches,
                "native_fnv_digest": fnv_digest(native),
                "pyscriptc_recomputed_fnv_digest": fnv_digest(ps),
                "native_vote_decisions": "".join(map(str, native_vote)),
                "pyscriptc_vote_decisions": "".join(map(str, ps_vote)),
                "vote_counting_method": (
                    "fail-open sliding gate: mute only when all last "
                    f"{args.vote_window} similarities are below {args.threshold}; "
                    "history resets per input file and per repeat"
                ),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
