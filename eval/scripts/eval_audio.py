#!/usr/bin/env python3
"""
Evaluate Voce's speaker-gate filter using multi-speaker AliMeeting test fixtures.

Workflow:
  1. Read manifest.json to discover test cases.
  2. For each test case:
     a. Enroll using the per-test enrollment WAV for the designated speaker.
     b. Run voce --eval on the test WAV with the enrollment embedding.
     c. Match JSONL decisions to ground-truth labels.
     d. Compute TP / FP / TN / FN and similarity statistics.
  3. Print per-test table + aggregate summary.
  4. Exit 0 if aggregate FPR < 5% AND Recall > 85%, else exit 1.

Usage:
    python eval/scripts/eval_audio.py [--binary ./target/release/voce]
                                      [--fixtures-dir eval/fixtures]
                                      [--threshold 0.85]
"""

import argparse
import json
import math
import pathlib
import subprocess
import sys
import tempfile

import numpy as np
import soundfile as sf

DEFAULT_FIXTURES = pathlib.Path("eval/fixtures")
WINDOW_S = 1.0  # 1-second analysis windows matching EmbeddingWindowAccumulator
TARGET_SR = 22050
TOLERANCE_S = 0.1  # ±100 ms for matching JSONL t_s to GT t_s


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "--binary",
        default="target/release/voce",
        help="Path to voce binary (default: target/release/voce)",
    )
    p.add_argument(
        "--fixtures-dir",
        type=pathlib.Path,
        default=DEFAULT_FIXTURES,
        help=f"Fixtures root (default: {DEFAULT_FIXTURES})",
    )
    p.add_argument(
        "--threshold",
        type=float,
        default=None,
        help="Override cosine similarity threshold by patching ~/.voce/config.json "
        "(restored afterwards)",
    )
    p.add_argument(
        "--fpr-limit",
        type=float,
        default=0.05,
        help="Pass criterion: aggregate FPR must be below this value (default: 0.05)",
    )
    p.add_argument(
        "--recall-min",
        type=float,
        default=0.85,
        help="Pass criterion: aggregate recall must exceed this value (default: 0.85)",
    )
    return p.parse_args()


# ---------------------------------------------------------------------------
# Config patching for threshold override
# ---------------------------------------------------------------------------


def patch_config(threshold: float) -> str | None:
    config_path = pathlib.Path.home() / ".voce" / "config.json"
    original = config_path.read_text() if config_path.exists() else None
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text(json.dumps({"threshold": threshold, "vote_window": 3}))
    return original


def restore_config(original: str | None) -> None:
    config_path = pathlib.Path.home() / ".voce" / "config.json"
    if original is None:
        config_path.unlink(missing_ok=True)
    else:
        config_path.write_text(original)


# ---------------------------------------------------------------------------
# Manifest loading
# ---------------------------------------------------------------------------


def load_manifest(tests_dir: pathlib.Path) -> list[dict]:
    """
    Load manifest.json from the AliMeeting tests directory.
    Returns list of test case dicts with resolved absolute paths.
    """
    manifest_path = tests_dir / "manifest.json"
    if not manifest_path.exists():
        print(
            f"ERROR: {manifest_path} not found.\nRun eval fixture generation first.",
            file=sys.stderr,
        )
        sys.exit(1)

    with open(manifest_path) as f:
        manifest = json.load(f)

    test_cases = manifest.get("test_cases", [])
    if not test_cases:
        print(
            f"ERROR: manifest.json contains no test_cases.\n"
            "Run eval fixture generation first.",
            file=sys.stderr,
        )
        sys.exit(1)

    # Validate and resolve paths
    resolved = []
    for tc in test_cases:
        enroll_path = tests_dir / tc["enrollment_file"]
        test_wav_path = tests_dir / tc["test_wav"]
        gt_path = tests_dir / tc["gt_jsonl"]

        if not enroll_path.exists():
            print(f"ERROR: enrollment file not found: {enroll_path}", file=sys.stderr)
            sys.exit(1)
        if not test_wav_path.exists():
            print(f"ERROR: test WAV not found: {test_wav_path}", file=sys.stderr)
            sys.exit(1)
        if not gt_path.exists():
            print(f"ERROR: GT JSONL not found: {gt_path}", file=sys.stderr)
            sys.exit(1)

        resolved.append(
            {
                "test_id": tc["test_id"],
                "source_file": tc.get("source_file", ""),
                "enrolled_speaker": tc.get("enrolled_speaker", ""),
                "enrollment_path": enroll_path,
                "test_wav_path": test_wav_path,
                "gt_path": gt_path,
            }
        )

    return resolved


# ---------------------------------------------------------------------------
# Voce binary interaction
# ---------------------------------------------------------------------------


def run_enroll(binary: str, wav_path: pathlib.Path, out_path: pathlib.Path) -> None:
    result = subprocess.run(
        [binary, "--eval-enroll", str(wav_path), "--eval-enroll-out", str(out_path)],
        capture_output=True,
        text=True,
        timeout=120,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"voce --eval-enroll failed (exit {result.returncode}):\n{result.stderr[:400]}"
        )


def run_eval_with_output(
    binary: str,
    wav_path: pathlib.Path,
    enrollment_path: pathlib.Path,
    output_path: pathlib.Path,
) -> str:
    """Run voce --eval, write filtered WAV, return JSONL stdout."""
    result = subprocess.run(
        [
            binary,
            "--eval",
            str(wav_path),
            "--enrollment",
            str(enrollment_path),
            "--output",
            str(output_path),
        ],
        capture_output=True,
        text=True,
        timeout=300,
    )
    if result.returncode != 0:
        raise RuntimeError(
            f"voce --eval failed (exit {result.returncode}):\n{result.stderr[:400]}"
        )
    return result.stdout


# ---------------------------------------------------------------------------
# JSONL parsing and similarity statistics
# ---------------------------------------------------------------------------


def parse_jsonl(text: str) -> list[dict]:
    rows = []
    for line in text.splitlines():
        line = line.strip()
        if line:
            rows.append(json.loads(line))
    return rows


def similarity_stats(jsonl_rows: list[dict], gt_rows: list[dict]) -> dict:
    """
    Match JSONL rows to GT rows by nearest t_s within ±TOLERANCE_S.
    Returns enrolled_sims and other_sims lists of similarity floats.
    """
    pred_by_t = {row["t_s"]: row["similarity"] for row in jsonl_rows}

    enrolled_sims: list[float] = []
    other_sims: list[float] = []

    for gt_row in gt_rows:
        gt_t = gt_row["t_s"]
        candidates = [
            (abs(p_t - gt_t), p_t)
            for p_t in pred_by_t
            if abs(p_t - gt_t) <= TOLERANCE_S
        ]
        if not candidates:
            continue
        _, best_t = min(candidates)
        sim = pred_by_t[best_t]

        if gt_row["is_enrolled"]:
            enrolled_sims.append(sim)
        else:
            other_sims.append(sim)

    return {"enrolled": enrolled_sims, "other": other_sims}


def fmt_sim_stats(sims: list[float], label: str) -> str:
    if not sims:
        return f"{label}: N/A"
    return (
        f"{label}: mean={sum(sims) / len(sims):.3f} "
        f"min={min(sims):.3f} max={max(sims):.3f}"
    )


# ---------------------------------------------------------------------------
# Audio loading (kept for writing the filtered WAV artifact)
# ---------------------------------------------------------------------------


def load_mono_f32(path: pathlib.Path) -> tuple[np.ndarray, int]:
    """Load WAV as mono float32, return (samples, sample_rate)."""
    data, sr = sf.read(str(path), dtype="float32", always_2d=False)
    if data.ndim > 1:
        data = data.mean(axis=1)
    return data.astype(np.float32), sr


# ---------------------------------------------------------------------------
# Per-window comparison using gate decisions from JSONL
# ---------------------------------------------------------------------------


def compare_jsonl(jsonl_rows: list[dict], gt_rows: list[dict]) -> dict[str, int]:
    """
    Match each GT window to the closest JSONL prediction within ±TOLERANCE_S.
    Uses the binary's own pass/fail decisions (not energy ratios).
    Returns {tp, fp, tn, fn}.
    """
    pred_by_t = {row["t_s"]: bool(row["passed"]) for row in jsonl_rows}
    matched_pred_ts: set[float] = set()

    tp = fp = tn = fn = 0

    for gt_row in gt_rows:
        gt_t = gt_row["t_s"]
        gt_enrolled = bool(gt_row["is_enrolled"])

        candidates = [
            (abs(p_t - gt_t), p_t)
            for p_t in pred_by_t
            if abs(p_t - gt_t) <= TOLERANCE_S
        ]

        if not candidates:
            # No prediction for this window — FN if enrolled, TN if other
            if gt_enrolled:
                fn += 1
            else:
                tn += 1
            continue

        _, best_t = min(candidates)
        matched_pred_ts.add(best_t)
        pred_passed = pred_by_t[best_t]

        if gt_enrolled and pred_passed:
            tp += 1
        elif gt_enrolled and not pred_passed:
            fn += 1
        elif not gt_enrolled and pred_passed:
            fp += 1
        else:
            tn += 1

    # Unmatched predictions that passed → FP
    for p_t, pred_passed in pred_by_t.items():
        if p_t not in matched_pred_ts and pred_passed:
            fp += 1

    return {"tp": tp, "fp": fp, "tn": tn, "fn": fn}


# ---------------------------------------------------------------------------
# Metrics
# ---------------------------------------------------------------------------


def metrics(c: dict[str, int]) -> dict[str, float]:
    tp, fp, tn, fn = c["tp"], c["fp"], c["tn"], c["fn"]
    precision = tp / (tp + fp) if (tp + fp) > 0 else float("nan")
    recall = tp / (tp + fn) if (tp + fn) > 0 else float("nan")
    if math.isnan(precision) or math.isnan(recall) or (precision + recall) == 0:
        f1 = float("nan")
    else:
        f1 = 2 * precision * recall / (precision + recall)
    fpr = fp / (fp + tn) if (fp + tn) > 0 else float("nan")
    return {"precision": precision, "recall": recall, "f1": f1, "fpr": fpr}


def fmt(v: float, fmt_str: str = ".3f") -> str:
    return "N/A" if math.isnan(v) else format(v, fmt_str)


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------


def main() -> None:
    args = parse_args()
    tests_dir: pathlib.Path = args.fixtures_dir / "alimeeting" / "tests"

    if not tests_dir.is_dir():
        print(
            f"ERROR: {tests_dir} not found.\nRun eval fixture generation first.",
            file=sys.stderr,
        )
        sys.exit(1)

    # Load manifest and validate all file paths
    test_cases = load_manifest(tests_dir)

    print(f"Loaded {len(test_cases)} test cases from manifest")

    # Optionally patch config for threshold override
    original_config = None
    if args.threshold is not None:
        original_config = patch_config(args.threshold)
        print(f"Threshold overridden to {args.threshold} (config patched)")

    try:
        _run_eval(args, tests_dir, test_cases)
    finally:
        if args.threshold is not None:
            restore_config(original_config)


def _run_eval(
    args: argparse.Namespace,
    tests_dir: pathlib.Path,
    test_cases: list[dict],
) -> None:
    with tempfile.TemporaryDirectory() as tmpdir:
        tmpdir = pathlib.Path(tmpdir)

        # Report table setup
        col = 20
        col_spk = 14
        header = (
            f"{'Test ID':<{col}} {'Speaker':<{col_spk}}"
            f" {'TP':>4} {'FP':>4} {'TN':>4} {'FN':>4}"
            f"  {'FPR':>6}  {'Rec':>6}  {'F1':>6}"
        )
        sep = "─" * len(header)
        print(f"\nVOCE AUDIO EVAL REPORT  (binary: {args.binary})")
        print("═" * len(header))
        print(header)
        print(sep)

        total = {"tp": 0, "fp": 0, "tn": 0, "fn": 0}
        any_failure = False
        # Accumulate similarity stats across all tests for aggregate
        agg_enrolled_sims: list[float] = []
        agg_other_sims: list[float] = []

        for tc in test_cases:
            test_id = tc["test_id"]
            speaker = tc["enrolled_speaker"]
            enroll_path = tc["enrollment_path"]
            test_wav_path = tc["test_wav_path"]
            gt_path = tc["gt_path"]

            # Per-test enrollment embedding (each test may enroll a different speaker)
            enroll_out = tmpdir / f"emb_{test_id}.json"

            try:
                # Step 1: enroll from per-test enrollment WAV
                run_enroll(args.binary, enroll_path, enroll_out)

                # Step 2: run eval on test WAV
                filtered_path = tmpdir / f"filtered_{test_id}.wav"
                jsonl_stdout = run_eval_with_output(
                    args.binary, test_wav_path, enroll_out, filtered_path
                )

                # Step 3: parse GT and predictions
                gt_rows = []
                with open(gt_path) as f:
                    for line in f:
                        line = line.strip()
                        if line:
                            gt_rows.append(json.loads(line))

                jsonl_rows = parse_jsonl(jsonl_stdout)
                c = compare_jsonl(jsonl_rows, gt_rows)
                m = metrics(c)
                sims = similarity_stats(jsonl_rows, gt_rows)

            except Exception as exc:
                print(f"  ERROR [{test_id}]: {exc}", file=sys.stderr)
                any_failure = True
                continue

            for k in total:
                total[k] += c[k]

            agg_enrolled_sims.extend(sims["enrolled"])
            agg_other_sims.extend(sims["other"])

            print(
                f"{test_id:<{col}} {speaker:<{col_spk}}"
                f" {c['tp']:>4} {c['fp']:>4} {c['tn']:>4} {c['fn']:>4}"
                f"  {fmt(m['fpr'], '.1%'):>6}  {fmt(m['recall'], '.1%'):>6}  {fmt(m['f1']):>6}"
            )
            # Per-test similarity summary
            enr_stat = fmt_sim_stats(sims["enrolled"], "enrolled")
            oth_stat = fmt_sim_stats(sims["other"], "other")
            if sims["enrolled"] or sims["other"]:
                print(f"  {'':<{col}} sim {enr_stat} | {oth_stat}")

        # Aggregate
        tm = metrics(total)
        print(sep)
        print(
            f"{'AGGREGATE':<{col}} {'':<{col_spk}}"
            f" {total['tp']:>4} {total['fp']:>4}"
            f" {total['tn']:>4} {total['fn']:>4}"
            f"  {fmt(tm['fpr'], '.1%'):>6}  {fmt(tm['recall'], '.1%'):>6}  {fmt(tm['f1']):>6}"
        )
        # Aggregate similarity stats
        agg_enr_stat = fmt_sim_stats(agg_enrolled_sims, "enrolled")
        agg_oth_stat = fmt_sim_stats(agg_other_sims, "other")
        print(f"  {'':<{col}} sim {agg_enr_stat} | {agg_oth_stat}")
        print("═" * len(header))

        # Pass criteria
        fpr_ok = (not math.isnan(tm["fpr"])) and tm["fpr"] < args.fpr_limit
        rec_ok = (not math.isnan(tm["recall"])) and tm["recall"] > args.recall_min
        print("\nPASS CRITERIA")
        print(
            f"  FPR  < {args.fpr_limit:.0%}:  "
            + ("\u2705" if fpr_ok else "\u274c")
            + f"  {fmt(tm['fpr'], '.1%')}"
            + ("" if fpr_ok else "  \u2014 try raising threshold")
        )
        print(
            f"  Recall > {args.recall_min:.0%}:  "
            + ("\u2705" if rec_ok else "\u274c")
            + f"  {fmt(tm['recall'], '.1%')}"
            + ("" if rec_ok else "  \u2014 try lowering threshold")
        )
        overall_ok = fpr_ok and rec_ok and not any_failure
        overall_label = "\u2705 PASS" if overall_ok else "\u274c FAIL"
        print(f"  Overall:  {overall_label}\n")

        sys.exit(0 if overall_ok else 1)


if __name__ == "__main__":
    main()
