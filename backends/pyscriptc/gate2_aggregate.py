#!/usr/bin/env python3
"""Aggregate paired native-probe and pyscriptc Gate 2 JSON reports.

Reads two native reports (native-run-1.json, native-run-2.json, each with two
repeats) and two pyscriptc reports per WAV (ps-NAME-1.json, ps-NAME-2.json,
each `--repeat 1`) from one directory and writes aggregate.json with parity,
recall/FPR, timing, RSS, and digest results. Measurement-only helper; it does
not change either CLI.

Vote counting is normalized across lanes. Both probes emit raw ABI decisions,
so the canonical app gate from `src/filter/gate.rs` is applied externally to
both sides: fail-open, mute only when all last 3 similarities are below
threshold. Vote history resets at the start of every input file and every
repeat; repeats are separate observations and are never chained into one
stream. Each 3-second window is counted exactly once in its own file/repeat.
Repeats are aggregated only by summing numerator and denominator, so the rate
is unchanged by the number of repeats.
"""

import argparse
import json
import statistics
import struct
from pathlib import Path

INPUTS = ("target_1272", "other_2035", "other_5895", "other_777")
THRESHOLD = 0.75
VOTE_WINDOW = 3
VOTE_METHOD = (
    "canonical SlidingVoteGate (src/filter/gate.rs): fail-open, mute only when "
    "all last 3 similarities are < 0.75; vote history resets per input file and "
    "per repeat; each 3 s window counted once per file per repeat; repeats "
    "aggregated by summed counts, never chained"
)


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


def vote(windows: list[dict], threshold: float = THRESHOLD, size: int = VOTE_WINDOW) -> list[int]:
    """Per-file/per-repeat fail-open vote gate; no state carried across calls."""
    recent: list[bool] = []
    out: list[int] = []
    for window in windows:
        recent.append(float(window["similarity"]) >= threshold)
        recent = recent[-size:]
        muted = len(recent) == size and not any(recent)
        out.append(0 if muted else 1)
    return out


def load(path: Path) -> dict:
    return json.loads(path.read_text())


def native_by_name(report: dict, run_index: int) -> dict[str, dict]:
    result: dict[str, dict] = {}
    for item in report["runs"][run_index]["inputs"]:
        result[Path(item["path"]).stem] = item
    return result


def load_native_runs(root: Path, repeats: int) -> list[tuple[dict, int]]:
    """One report with `--repeat N`, or one single-run report per repeat."""
    paths = [p for i in (1, 2) if (p := root / f"native-run-{i}.json").exists()]
    if len(paths) == 2:
        return [(load(path), 0) for path in paths]
    if len(paths) == 1:
        report = load(paths[0])
        if report["repeat_count"] >= repeats:
            return [(report, index) for index in range(repeats)]
    raise SystemExit(f"need {repeats} native repeats in {root}")


def load_ps_reports(root: Path, name: str, repeats: int) -> list[dict]:
    """Two single-repeat invocations, or one invocation with `--repeat N`."""
    paths = [p for i in (1, 2) if (p := root / f"ps-{name}-{i}.json").exists()]
    if len(paths) == 2:
        return [load(path) for path in paths]
    if len(paths) == 1:
        report = load(paths[0])
        if report["repeat"] >= repeats:
            return [report] * repeats
    raise SystemExit(f"need {repeats} pyscriptc repeats for {name} in {root}")


def rate(passed: int, total: int) -> float:
    return passed / total if total else 0.0


def counts(windows: list[dict]) -> tuple[int, int, int]:
    raw = sum(int(w["decision"]) for w in windows)
    gated = sum(vote(windows))
    return len(windows), raw, gated


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dir", required=True, help="directory with paired probe JSON")
    parser.add_argument("--threshold", type=float, default=THRESHOLD)
    parser.add_argument("--vote-window", type=int, default=VOTE_WINDOW)
    parser.add_argument("--repeats", type=int, default=2, help="number of independent repeats per file")
    args = parser.parse_args()
    root = Path(args.dir)
    repeats = args.repeats
    native_runs = load_native_runs(root, repeats)
    ps = {name: load_ps_reports(root, name, repeats) for name in INPUTS}

    raw = vote_mismatch = sim_mismatch = fnv_mismatch = 0
    per_repeat = []
    for rep in range(repeats):
        nb = native_by_name(*native_runs[rep])
        rep_row = {
            "repeat": rep + 1,
            "target_windows": 0,
            "target_raw_pass_native": 0,
            "target_raw_pass_pyscriptc": 0,
            "target_vote_pass_native": 0,
            "target_vote_pass_pyscriptc": 0,
            "other_windows": 0,
            "other_raw_pass_native": 0,
            "other_raw_pass_pyscriptc": 0,
            "other_vote_pass_native": 0,
            "other_vote_pass_pyscriptc": 0,
        }
        for name in INPUTS:
            ref = nb[name]["windows"]
            windows = ps[name][rep]["windows"]
            raw += sum(
                1 for a, b in zip(ref, windows) if a["decision"] != b["decision"]
            )
            vote_mismatch += sum(
                1 for a, b in zip(vote(ref, args.threshold, args.vote_window),
                                  vote(windows, args.threshold, args.vote_window))
                if a != b
            )
            sim_mismatch += sum(
                1
                for a, b in zip(ref, windows)
                if struct.pack("<f", a["similarity"]) != struct.pack("<f", b["similarity"])
            )
            fnv_mismatch += int(fnv(windows) != nb[name]["digest"])

            nwin, nraw, nvote = counts(ref)
            pwin, praw, pvote = counts(windows)
            key = "target" if name == "target_1272" else "other"
            rep_row[f"{key}_windows"] += nwin
            rep_row[f"{key}_raw_pass_native"] += nraw
            rep_row[f"{key}_raw_pass_pyscriptc"] += praw
            rep_row[f"{key}_vote_pass_native"] += nvote
            rep_row[f"{key}_vote_pass_pyscriptc"] += pvote
        rep_row["target_recall_raw_native"] = rate(
            rep_row["target_raw_pass_native"], rep_row["target_windows"]
        )
        rep_row["target_recall_raw_pyscriptc"] = rate(
            rep_row["target_raw_pass_pyscriptc"], rep_row["target_windows"]
        )
        rep_row["target_recall_vote_native"] = rate(
            rep_row["target_vote_pass_native"], rep_row["target_windows"]
        )
        rep_row["target_recall_vote_pyscriptc"] = rate(
            rep_row["target_vote_pass_pyscriptc"], rep_row["target_windows"]
        )
        rep_row["other_fpr_raw_native"] = rate(
            rep_row["other_raw_pass_native"], rep_row["other_windows"]
        )
        rep_row["other_fpr_raw_pyscriptc"] = rate(
            rep_row["other_raw_pass_pyscriptc"], rep_row["other_windows"]
        )
        rep_row["other_fpr_vote_native"] = rate(
            rep_row["other_vote_pass_native"], rep_row["other_windows"]
        )
        rep_row["other_fpr_vote_pyscriptc"] = rate(
            rep_row["other_vote_pass_pyscriptc"], rep_row["other_windows"]
        )
        per_repeat.append(rep_row)

    pooled = {
        f"{side}_{metric}": sum(row[f"{side}_{metric}"] for row in per_repeat)
        for side in ("target", "other")
        for metric in (
            "windows", "raw_pass_native", "raw_pass_pyscriptc",
            "vote_pass_native", "vote_pass_pyscriptc",
        )
    }
    for metric in (
        "target_recall_raw_native", "target_recall_raw_pyscriptc",
        "target_recall_vote_native", "target_recall_vote_pyscriptc",
        "other_fpr_raw_native", "other_fpr_raw_pyscriptc",
        "other_fpr_vote_native", "other_fpr_vote_pyscriptc",
    ):
        prefix, side = metric.split("_", 1)
        key = "target" if prefix == "target" else "other"
        num = "raw_pass" if "raw" in metric else "vote_pass"
        backend = "native" if metric.endswith("native") else "pyscriptc"
        pooled[metric] = rate(pooled[f"{key}_{num}_{backend}"], pooled[f"{key}_windows"])

    deterministic = all(
        report["deterministic"] for name in INPUTS for report in ps[name]
    ) and all(report["deterministic"] for report, _ in native_runs)

    native_mean = statistics.mean(
        report["runs"][run_index]["runtime_ms_per_window"]
        for report, run_index in native_runs
    )
    ps_mean = statistics.mean(
        report["runtime_ms_total"] / report["window_count"]
        for name in INPUTS
        for report in ps[name]
    )
    native_rss = max(int(report["rss_peak_bytes"]) for report, _ in native_runs)
    ps_rss = max(report["rss_peak_kb"] * 1024 for name in INPUTS for report in ps[name])

    # Canonical summary fields use pyscriptc's pooled observations. Native
    # counterparts are included in `pooled` for direction-independent review.
    out = {
        "vote_counting_method": VOTE_METHOD,
        "vote_state_reset_per_file": True,
        "vote_state_reset_per_repeat": True,
        "repeats_double_counted": False,
        "windows_per_file_per_repeat": per_repeat[0]["target_windows"],
        "raw_decision_mismatches": raw,
        "vote_gated_decision_mismatches": vote_mismatch,
        "windows_compared": repeats * len(INPUTS) * per_repeat[0]["target_windows"],
        "similarity_mismatches": sim_mismatch,
        "fnv_digest_mismatches": fnv_mismatch,
        "deterministic": deterministic,
        "target_recall_raw": pooled["target_recall_raw_pyscriptc"],
        "target_recall_vote": pooled["target_recall_vote_pyscriptc"],
        "other_fpr_raw": pooled["other_fpr_raw_pyscriptc"],
        "other_fpr_vote": pooled["other_fpr_vote_pyscriptc"],
        "target_pass": pooled["target_raw_pass_pyscriptc"],
        "target_windows": pooled["target_windows"],
        "other_pass": pooled["other_raw_pass_pyscriptc"],
        "other_windows": pooled["other_windows"],
        "target_vote_pass": pooled["target_vote_pass_pyscriptc"],
        "other_vote_pass": pooled["other_vote_pass_pyscriptc"],
        "per_repeat": per_repeat,
        "pooled": pooled,
        "native_mean_window_ms": native_mean,
        "pyscriptc_mean_window_ms": ps_mean,
        "native_peak_rss_bytes": native_rss,
        "pyscriptc_peak_rss_bytes": ps_rss,
        "artifact_bytes": ps["target_1272"][0]["artifact_size_bytes"],
        "native_digests": [
            report["runs"][run_index]["digest"] for report, run_index in native_runs
        ],
        "pyscriptc_recomputed_digests": {n: fnv(ps[n][0]["windows"]) for n in INPUTS},
        "native_deterministic": [report["deterministic"] for report, _ in native_runs],
        "native_runs": [
            {
                "runtime_ms_per_window": report["runs"][run_index]["runtime_ms_per_window"],
                "peak_rss_bytes": report["rss_peak_bytes"],
                "model_load_ms": report["runs"][run_index]["model_load_ms"],
            }
            for report, run_index in native_runs
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
