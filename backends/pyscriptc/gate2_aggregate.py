#!/usr/bin/env python3
"""Aggregate paired native-probe and pyscriptc Gate 2 JSON reports.

Reads two native reports (native-run-1.json, native-run-2.json) and two
pyscriptc reports per WAV (ps-NAME-1.json, ps-NAME-2.json) from one directory
and writes aggregate.json with parity, recall/FPR, timing, RSS, and digest
results. Measurement-only helper; it does not change either CLI.
"""

import argparse
import json
import statistics
import struct
from pathlib import Path

INPUTS = ("target_1272", "other_2035", "other_5895", "other_777")


def fnv(windows: list[dict]) -> str:
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
    out: list[int] = []
    for window in windows:
        recent.append(1 if window["decision"] == 1 else 0)
        recent = recent[-size:]
        out.append(1 if sum(recent) * 2 >= size else 0)
    return out


def load(path: Path) -> dict:
    return json.loads(path.read_text())


def native_by_name(report: dict) -> dict[str, dict]:
    result: dict[str, dict] = {}
    for run in report["runs"]:
        for item in run["inputs"]:
            result[Path(item["path"]).stem] = item
    return result


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dir", required=True, help="directory with paired probe JSON")
    args = parser.parse_args()
    root = Path(args.dir)
    native = [load(root / f"native-run-{i}.json") for i in (1, 2)]
    ps = {name: [load(root / f"ps-{name}-{i}.json") for i in (1, 2)] for name in INPUTS}

    nb = native_by_name(native[0])
    raw = vote_mismatch = sim_mismatch = fnv_mismatch = 0
    for name in INPUTS:
        reference = nb[name]["windows"]
        for report in ps[name]:
            windows = report["windows"]
            raw += sum(1 for a, b in zip(reference, windows) if a["decision"] != b["decision"])
            vote_mismatch += sum(1 for a, b in zip(vote(reference), vote(windows)) if a != b)
            sim_mismatch += sum(
                1
                for a, b in zip(reference, windows)
                if struct.pack("<f", a["similarity"]) != struct.pack("<f", b["similarity"])
            )
            fnv_mismatch += int(fnv(windows) != nb[name]["digest"])

    first = {name: ps[name][0] for name in INPUTS}
    second = {name: ps[name][1] for name in INPUTS}
    deterministic = all(
        [w["similarity"] for w in first[n]["windows"]]
        == [w["similarity"] for w in second[n]["windows"]]
        and [w["decision"] for w in first[n]["windows"]]
        == [w["decision"] for w in second[n]["windows"]]
        and first[n]["decision_digest"] == second[n]["decision_digest"]
        for n in INPUTS
    )
    target_windows = first["target_1272"]["window_count"]
    other_windows = sum(first[n]["window_count"] for n in INPUTS if n != "target_1272")
    target_pass = first["target_1272"]["passed_window_count"]
    other_pass = sum(first[n]["passed_window_count"] for n in INPUTS if n != "target_1272")
    target_vote = sum(vote(first["target_1272"]["windows"]))
    other_vote = sum(sum(vote(first[n]["windows"])) for n in INPUTS if n != "target_1272")

    native_mean = statistics.mean(r["runs"][0]["runtime_ms_per_window"] for r in native)
    ps_mean = statistics.mean(
        report["runtime_ms_total"] / report["window_count"]
        for name in INPUTS
        for report in ps[name]
    )
    native_rss = max(int(r["rss_peak_bytes"]) for r in native)
    ps_rss = max(report["rss_peak_kb"] * 1024 for name in INPUTS for report in ps[name])

    out = {
        "raw_decision_mismatches": raw,
        "vote_gated_decision_mismatches": vote_mismatch,
        "similarity_mismatches": sim_mismatch,
        "fnv_digest_mismatches": fnv_mismatch,
        "deterministic": deterministic,
        "target_recall_raw": target_pass / target_windows,
        "target_recall_vote": target_vote / target_windows,
        "other_fpr_raw": other_pass / other_windows,
        "other_fpr_vote": other_vote / other_windows,
        "target_pass": target_pass,
        "target_windows": target_windows,
        "other_pass": other_pass,
        "other_windows": other_windows,
        "target_vote_pass": target_vote,
        "other_vote_pass": other_vote,
        "native_mean_window_ms": native_mean,
        "pyscriptc_mean_window_ms": ps_mean,
        "native_peak_rss_bytes": native_rss,
        "pyscriptc_peak_rss_bytes": ps_rss,
        "artifact_bytes": first["target_1272"]["artifact_size_bytes"],
        "native_digests": [r["runs"][0]["digest"] for r in native],
        "pyscriptc_recomputed_digests": {n: fnv(ps[n][0]["windows"]) for n in INPUTS},
        "native_deterministic": [r["deterministic"] for r in native],
        "native_runs": [
            {
                "runtime_ms_per_window": r["runs"][0]["runtime_ms_per_window"],
                "peak_rss_bytes": r["rss_peak_bytes"],
                "model_load_ms": r["runs"][0]["model_load_ms"],
            }
            for r in native
        ],
        "pyscriptc_runs": {
            n: [
                {
                    "mean_window_ms": r["runtime_ms_total"] / r["window_count"],
                    "wall_ms_total": r["wall_ms_total"],
                    "peak_rss_bytes": r["rss_peak_kb"] * 1024,
                    "model_load_ms": r["model_load_ms"],
                    "passed": r["passed_window_count"],
                    "decision_digest": r["decision_digest"],
                    "roundtrip": r["profile_round_trip"],
                }
                for r in ps[n]
            ]
            for n in INPUTS
        },
    }
    (root / "aggregate.json").write_text(json.dumps(out, indent=2) + "\n")
    print(json.dumps(out, indent=2))


if __name__ == "__main__":
    main()
