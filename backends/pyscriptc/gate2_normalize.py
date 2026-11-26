#!/usr/bin/env python3
"""Normalize Gate 2 probe output for native-vs-pyscriptc comparison.

Both probes emit raw per-window decisions. This helper applies the documented
majority vote rule to either side and recomputes the Rust FNV-1a digest from
pyscriptc window fields, so parity can be checked without changing either CLI.
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


def vote(windows: list[dict], size: int = 3) -> list[int]:
    recent: list[int] = []
    result: list[int] = []
    for window in windows:
        recent.append(1 if window["decision"] == 1 else 0)
        recent = recent[-size:]
        result.append(1 if sum(recent) * 2 >= size else 0)
    return result


def native_windows(report: dict, path: str) -> list[dict]:
    for run in report["runs"]:
        for item in run["inputs"]:
            if Path(item["path"]).name == Path(path).name:
                return item["windows"]
    raise SystemExit(f"no native input matching {path}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native", required=True, help="voce-native-probe JSON report")
    parser.add_argument("--pyscriptc", required=True, help="voce_spike JSON line")
    parser.add_argument("--input", required=True, help="WAV basename to compare")
    args = parser.parse_args()

    native = native_windows(json.loads(Path(args.native).read_text()), args.input)
    ps = json.loads(Path(args.pyscriptc).read_text())["windows"]

    raw_mismatches = sum(
        1 for a, b in zip(native, ps) if a["decision"] != b["decision"]
    )
    vote_mismatches = sum(1 for a, b in zip(vote(native), vote(ps)) if a != b)
    similarity_mismatches = sum(
        1
        for a, b in zip(native, ps)
        if struct.pack("<f", a["similarity"]) != struct.pack("<f", b["similarity"])
    )
    print(
        json.dumps(
            {
                "input": args.input,
                "native_window_count": len(native),
                "pyscriptc_window_count": len(ps),
                "raw_decision_mismatches": raw_mismatches,
                "vote_gated_decision_mismatches": vote_mismatches,
                "similarity_mismatches": similarity_mismatches,
                "native_fnv_digest": fnv_digest(native),
                "pyscriptc_recomputed_fnv_digest": fnv_digest(ps),
                "native_vote_decisions": "".join(map(str, vote(native))),
                "pyscriptc_vote_decisions": "".join(map(str, vote(ps))),
            }
        )
    )


if __name__ == "__main__":
    main()
