#!/usr/bin/env python3
"""Corrected Gate 2 fixture sweep for the scriptc native backend.

The original bakeoff helpers lived in /tmp and counted windows differently
across lanes. This harness is committed so both lanes can reuse one method:

* vote state is fresh for every input file and every repeat;
* each 3 s window is counted exactly once in that file/repeat;
* repeats are aggregated separately and never concatenated into one vote
  stream.

All builds and probe invocations are individually wrapped in `/tmp/tsc.lock`.
"""

import argparse
import hashlib
import json
import struct
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = Path("/home/own-agents/.cache/voce/gate2-fixtures")
MODEL = Path("/home/own-agents/.cache/voce/models/voxblink2_samresnet34_ft.onnx")
INPUTS = (
    "target_1272.wav",
    "other_2035.wav",
    "other_5895.wav",
    "other_777.wav",
)
THRESHOLD = 0.75
VOTE_WINDOW = 3
LOCK = "/tmp/tsc.lock"

VOTE_METHOD = (
    "per-file independent SlidingVoteGate state reset for each (input, repeat); "
    "each 3 s window counted once; per-repeat results aggregated separately "
    "without concatenating vote histories"
)


def locked(cmd: list[str], timeout: int = 1800) -> list[str]:
    return ["flock", "-w", str(timeout), LOCK, *cmd]


def run(name: str, cmd: list[str], evidence: Path) -> tuple[dict, float]:
    """Run one measured command under the shared lock and persist its output."""
    full_cmd = locked(cmd)
    started = time.monotonic_ns()
    proc = subprocess.run(
        full_cmd, cwd=str(ROOT), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    wall_ms = (time.monotonic_ns() - started) / 1e6
    (evidence / f"{name}.stdout").write_text(proc.stdout)
    (evidence / f"{name}.stderr").write_text(proc.stderr)
    if proc.returncode != 0:
        raise RuntimeError(f"{name} failed rc={proc.returncode}: {proc.stderr[:4000]}")
    return {"command": " ".join(full_cmd), "wall_ms": wall_ms, "stdout": proc.stdout}, wall_ms


def fnv(windows: list[dict]) -> str:
    h = 0xCBF29CE484222325
    for w in windows:
        for key in ("similarity", "speech_probability", "decision"):
            value = w[key]
            word = (
                struct.unpack("<I", struct.pack("<f", float(value)))[0]
                if key != "decision"
                else int(value)
            )
            h ^= word
            h = (h * 0x100000001B3) & 0xFFFFFFFFFFFFFFFF
    return f"{h:016x}"


def raw_mismatches(a: list[dict], b: list[dict]) -> int:
    return sum(
        1
        for x, y in zip(a, b)
        if any(
            struct.pack("<f", float(x[k])) != struct.pack("<f", float(y[k]))
            for k in ("similarity", "speech_probability")
        )
        or int(x["decision"]) != int(y["decision"])
    )


def vote(windows: list[dict], rule: str = "app_all_failed") -> list[bool]:
    """Fail-open gate matching src/filter/gate.rs, or the old majority rule.

    `app_all_failed` mutes only when the last three similarities all fail.
    `majority_of_three` is the old pyscriptc helper (pass if at least two of
    the last three raw decisions pass); it is retained only as a diagnostic.
    """
    history: list[bool] = []
    out: list[bool] = []
    for w in windows:
        passed = float(w["similarity"]) >= THRESHOLD
        if rule == "majority_of_three":
            passed = int(w["decision"]) == 1
        history.append(passed)
        if len(history) > VOTE_WINDOW:
            history.pop(0)
        if rule == "app_all_failed":
            out.append(not (len(history) == VOTE_WINDOW and not any(history)))
        else:
            out.append(sum(history) * 2 >= VOTE_WINDOW)
    return out


def counts(target: dict, other: dict, raw_key: str, vote_key: str) -> dict:
    target_total = target[raw_key]
    other_total = other[raw_key]
    return {
        "target_pass": target[vote_key],
        "target_total": target_total,
        "other_pass": other[vote_key],
        "other_total": other_total,
        "target_rate": target[vote_key] / target_total,
        "other_rate": other[vote_key] / other_total,
    }


def empty_bucket() -> dict:
    return {
        "windows": 0,
        "raw_pass": 0,
        "app_vote_pass": 0,
        "majority_vote_pass": 0,
    }


def add_bucket(bucket: dict, windows: list[dict]) -> None:
    bucket["windows"] += len(windows)
    bucket["raw_pass"] += sum(int(w["decision"]) == 1 for w in windows)
    bucket["app_vote_pass"] += sum(vote(windows, "app_all_failed"))
    bucket["majority_vote_pass"] += sum(vote(windows, "majority_of_three"))


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--native-probe", type=Path, required=True)
    parser.add_argument("--scriptc-exe", type=Path, required=True)
    parser.add_argument("--evidence-dir", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()

    evidence = args.evidence_dir
    evidence.mkdir(parents=True, exist_ok=True)
    native_exe = args.native_probe.resolve()
    native_cmd = [
        str(native_exe), "--backend", "real", "--models-dir", str(MODEL),
        "--enroll", str(FIXTURES / "enroll_1272.wav"),
        "--threshold", str(THRESHOLD), "--repeat", "2",
    ]
    for name in INPUTS:
        native_cmd += ["--input", str(FIXTURES / name)]
    native_cmd += ["--out", str(evidence / "native-probe-release.json")]
    native_result, native_wall = run("native-probe-release", native_cmd, evidence)
    native = json.loads((evidence / "native-probe-release.json").read_text())

    native_by_input: dict[str, list[dict]] = {}
    for rep, report in enumerate(native["runs"]):
        for item in report["inputs"]:
            native_by_input.setdefault(Path(item["path"]).name, []).append(
                {
                    **item,
                    "model_load_ms": report["model_load_ms"],
                    "repeat": rep + 1,
                    "native_report_mean_window_ms": report["runtime_ms_per_window"],
                }
            )

    scriptc: dict[str, list[dict]] = {}
    scriptc_walls: dict[str, list[float]] = {}
    commands: list[dict] = [{"name": "native-probe-release", **native_result}]
    for rep in (1, 2):
        for name in INPUTS:
            cmd = [
                str(args.scriptc_exe), "run", "--backend", "real",
                "--model", str(MODEL), "--enroll", str(FIXTURES / "enroll_1272.wav"),
                "--input", str(FIXTURES / name), "--threshold", str(THRESHOLD),
            ]
            label = f"scriptc-release-{rep}-{Path(name).stem}"
            result, wall = run(label, cmd, evidence)
            payload = json.loads(result["stdout"])
            commands.append({"name": label, "command": result["command"], "wall_ms": wall})
            scriptc.setdefault(name, []).append(payload)
            scriptc_walls.setdefault(name, []).append(wall)

    per_input_repeat: list[dict] = []
    per_repeat = {str(rep): {"target": empty_bucket(), "other": empty_bucket()} for rep in (1, 2)}
    combined = {"target": empty_bucket(), "other": empty_bucket()}
    for rep in (1, 2):
        for name in INPUTS:
            n = native_by_input[name][rep - 1]
            s = scriptc[name][rep - 1]
            nw, sw = n["windows"], s["windows"]
            nvote, svote = vote(nw), vote(sw)
            bucket = per_repeat[str(rep)]["target" if name.startswith("target") else "other"]
            add_bucket(bucket, sw)
            add_bucket(combined["target" if name.startswith("target") else "other"], sw)
            per_input_repeat.append(
                {
                    "input": name,
                    "repeat": rep,
                    "native_windows": len(nw),
                    "scriptc_windows": len(sw),
                    "native_digest": n["digest"],
                    "scriptc_fnv_digest": fnv(sw),
                    "raw_mismatches": raw_mismatches(nw, sw),
                    "vote_mismatches": sum(a != b for a, b in zip(nvote, svote)),
                    "scriptc_raw_pass": sum(int(w["decision"]) == 1 for w in sw),
                    "scriptc_app_vote_pass": sum(svote),
                    "scriptc_majority_vote_pass": sum(vote(sw, "majority_of_three")),
                    "scriptc_mean_window_ms": s["mean_window_time_ms"],
                    "native_mean_window_ms": n["native_report_mean_window_ms"],
                    "scriptc_model_load_ms": s["model_load_ms"],
                    "native_model_load_ms": n["model_load_ms"],
                    "scriptc_rss_bytes": s["rss_kib"] * 1024,
                    "scriptc_wall_ms": scriptc_walls[name][rep - 1],
                }
            )

    digest_matches = sum(r["native_digest"] == r["scriptc_fnv_digest"] for r in per_input_repeat)
    deterministic = all(
        fnv(scriptc[name][0]["windows"]) == fnv(scriptc[name][1]["windows"])
        and json.dumps(scriptc[name][0]["profile_json"], sort_keys=True)
        == json.dumps(scriptc[name][1]["profile_json"], sort_keys=True)
        for name in INPUTS
    ) and bool(native["deterministic"])

    scriptc_mean = sum(r["scriptc_mean_window_ms"] for r in per_input_repeat) / len(per_input_repeat)
    native_mean = sum(r["native_mean_window_ms"] for r in per_input_repeat) / len(per_input_repeat)
    profile_json = scriptc[INPUTS[0]][0]["profile_json"]
    profile_bytes = len(json.dumps(profile_json, separators=(",", ":")))

    summary = {
        "vote_method": VOTE_METHOD,
        "vote_rule": "app_all_failed_fail_open (src/filter/gate.rs)",
        "threshold": THRESHOLD,
        "vote_window": VOTE_WINDOW,
        "inputs": list(INPUTS),
        "commands": commands,
        "native": {
            "probe": str(native_exe),
            "wall_ms": native_wall,
            "rss_peak_bytes": native["rss_peak_bytes"],
            "deterministic": native["deterministic"],
            "mean_window_ms": native_mean,
            "model_load_ms": [r["model_load_ms"] for r in native["runs"]],
            "profile_json_bytes": native["runs"][0]["profile_json_bytes"],
        },
        "scriptc": {
            "executable": str(args.scriptc_exe),
            "rss_peak_bytes": max(r["scriptc_rss_bytes"] for r in per_input_repeat),
            "artifact_bytes": scriptc[INPUTS[0]][0]["artifact_bytes"],
            "mean_window_ms": scriptc_mean,
            "model_load_ms_mean": sum(
                r["scriptc_model_load_ms"] for r in per_input_repeat
            ) / len(per_input_repeat),
            "deterministic_across_repeats": deterministic,
            "profile_json_bytes": profile_bytes,
            "total_run_wall_ms": sum(sum(v) for v in scriptc_walls.values()),
        },
        "counts": {
            "per_repeat": {
                rep: {
                    "target_windows": b["target"]["windows"],
                    "target_raw_pass": b["target"]["raw_pass"],
                    "target_app_vote_pass": b["target"]["app_vote_pass"],
                    "target_majority_vote_pass": b["target"]["majority_vote_pass"],
                    "other_windows": b["other"]["windows"],
                    "other_raw_pass": b["other"]["raw_pass"],
                    "other_app_vote_pass": b["other"]["app_vote_pass"],
                    "other_majority_vote_pass": b["other"]["majority_vote_pass"],
                }
                for rep, b in per_repeat.items()
            },
            "combined": counts(combined["target"], combined["other"], "windows", "raw_pass"),
            "combined_app_vote": counts(combined["target"], combined["other"], "windows", "app_vote_pass"),
            "combined_majority_vote": counts(
                combined["target"], combined["other"], "windows", "majority_vote_pass"
            ),
        },
        "parity": {
            "raw_mismatches": sum(r["raw_mismatches"] for r in per_input_repeat),
            "vote_mismatches": sum(r["vote_mismatches"] for r in per_input_repeat),
            "digest_matches": digest_matches,
            "digest_total": len(per_input_repeat),
            "windows_compared": sum(r["native_windows"] for r in per_input_repeat),
        },
        "per_input_repeat": per_input_repeat,
        "evidence_dir": str(evidence),
    }
    args.out.write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({
        "counts": summary["counts"],
        "parity": summary["parity"],
        "scriptc_mean_window_ms": scriptc_mean,
        "native_mean_window_ms": native_mean,
    }, indent=2))


if __name__ == "__main__":
    main()
