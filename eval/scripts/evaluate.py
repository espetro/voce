#!/usr/bin/env python3
"""
Run voce --eval on all mixed fixtures and report precision / recall / F1 / FPR.

Discovers WAV files in eval/fixtures/ that have a matching .gt.jsonl sidecar,
runs the voce binary on each, matches per-window predictions to ground truth
using a ±0.1 s tolerance window, computes TP/FP/TN/FN, and prints a summary
table.

Pass criteria (aggregate):
  FPR  < 5 %    (false positive rate)
  Recall > 85 %

Exit code: 0 if all criteria met, 1 otherwise (so CI can use this directly).

Usage:
    python eval/scripts/evaluate.py --binary ./target/release/voce [--threshold 0.75]
                                    [--fixtures-dir eval/fixtures] [--fpr-limit 0.05]
                                    [--recall-min 0.85]
"""
import argparse
import json
import math
import pathlib
import subprocess
import sys

TOLERANCE_S = 0.1   # ±100 ms for matching prediction t_s to GT t_s
DEFAULT_FIXTURES = pathlib.Path("eval/fixtures")


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--binary", default="target/release/voce",
                   help="Path to the voce binary (default: target/release/voce)")
    p.add_argument("--threshold", type=float, default=None,
                   help="Override cosine similarity threshold by patching ~/.voce/config.json "
                        "before running (restored afterwards)")
    p.add_argument("--fixtures-dir", type=pathlib.Path, default=DEFAULT_FIXTURES,
                   help=f"Fixtures directory (default: {DEFAULT_FIXTURES})")
    p.add_argument("--fpr-limit", type=float, default=0.05,
                   help="Pass criterion: aggregate FPR must be below this value (default: 0.05)")
    p.add_argument("--recall-min", type=float, default=0.85,
                   help="Pass criterion: aggregate recall must exceed this value (default: 0.85)")
    return p.parse_args()


# ---------------------------------------------------------------------------
# Voce binary interaction
# ---------------------------------------------------------------------------

def run_voce_eval(binary: str, wav_path: pathlib.Path) -> list[dict]:
    """Run `voce --eval <wav>`, parse JSONL stdout, return list of prediction dicts."""
    result = subprocess.run(
        [binary, "--eval", str(wav_path)],
        capture_output=True,
        text=True,
        timeout=300,
    )
    if result.returncode != 0:
        stderr_snippet = result.stderr[:400].strip()
        raise RuntimeError(
            f"voce --eval exited {result.returncode}: {stderr_snippet}"
        )
    rows = []
    for lineno, line in enumerate(result.stdout.splitlines(), start=1):
        line = line.strip()
        if not line:
            continue
        try:
            rows.append(json.loads(line))
        except json.JSONDecodeError as exc:
            raise RuntimeError(f"invalid JSON on line {lineno}: {line!r}") from exc
    return rows


def load_gt(gt_path: pathlib.Path) -> list[dict]:
    rows = []
    with open(gt_path) as f:
        for lineno, line in enumerate(f, start=1):
            line = line.strip()
            if not line:
                continue
            try:
                rows.append(json.loads(line))
            except json.JSONDecodeError as exc:
                raise RuntimeError(
                    f"invalid JSON in {gt_path} line {lineno}: {line!r}"
                ) from exc
    return rows


# ---------------------------------------------------------------------------
# Matching + metrics
# ---------------------------------------------------------------------------

def match_predictions(
    preds: list[dict], gt: list[dict]
) -> dict[str, int]:
    """
    Match each GT window to the closest prediction within ±TOLERANCE_S.
    Returns {"tp": int, "fp": int, "tn": int, "fn": int}.
    """
    pred_by_t = {row["t_s"]: bool(row["passed"]) for row in preds}
    matched_pred_ts: set[float] = set()

    tp = fp = tn = fn = 0

    for gt_row in gt:
        gt_t = gt_row["t_s"]
        gt_enrolled = bool(gt_row["is_enrolled"])

        # Find closest prediction within tolerance
        candidates = [
            (abs(p_t - gt_t), p_t)
            for p_t in pred_by_t
            if abs(p_t - gt_t) <= TOLERANCE_S
        ]

        if not candidates:
            # No matching prediction — count as FN if enrolled, TN if other
            if gt_enrolled:
                fn += 1
            else:
                tn += 1
            continue

        _, best_t = min(candidates)
        matched_pred_ts.add(best_t)
        pred_enrolled = pred_by_t[best_t]

        if gt_enrolled and pred_enrolled:
            tp += 1
        elif gt_enrolled and not pred_enrolled:
            fn += 1
        elif not gt_enrolled and pred_enrolled:
            fp += 1
        else:
            tn += 1

    # Predictions with no GT match — count as FP if they passed
    for p_t, pred_enrolled in pred_by_t.items():
        if p_t not in matched_pred_ts and pred_enrolled:
            fp += 1

    return {"tp": tp, "fp": fp, "tn": tn, "fn": fn}


def metrics(counts: dict[str, int]) -> dict[str, float]:
    tp, fp, tn, fn = counts["tp"], counts["fp"], counts["tn"], counts["fn"]
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
# Config patching for threshold sweep
# ---------------------------------------------------------------------------

def patch_config(threshold: float) -> str | None:
    """
    Write threshold to ~/.voce/config.json and return the original contents
    (or None if the file did not exist).
    """
    import os
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
# Main
# ---------------------------------------------------------------------------

def main() -> None:
    args = parse_args()

    # Discover fixtures: all WAVs with a matching .gt.jsonl
    fixtures: pathlib.Path = args.fixtures_dir
    if not fixtures.is_dir():
        print(f"ERROR: fixtures directory not found: {fixtures}", file=sys.stderr)
        sys.exit(1)

    wav_files = sorted(
        p for p in fixtures.glob("*.wav")
        if p.with_suffix("").with_suffix(".gt.jsonl").exists()
    )
    if not wav_files:
        print(
            f"No fixture WAVs with .gt.jsonl sidecars found in {fixtures}.\n"
            "Run make eval-setup first.",
            file=sys.stderr,
        )
        sys.exit(1)

    # Optionally patch config for threshold override
    original_config = None
    if args.threshold is not None:
        original_config = patch_config(args.threshold)
        print(f"Threshold overridden to {args.threshold} (config patched)")

    try:
        _run_eval(args, wav_files)
    finally:
        if original_config is not None or args.threshold is not None:
            restore_config(original_config)


def _run_eval(args: argparse.Namespace, wav_files: list[pathlib.Path]) -> None:
    col_name = 38
    header = (
        f"{'File':<{col_name}} {'TP':>4} {'FP':>4} {'TN':>4} {'FN':>4}"
        f"  {'FPR':>6}  {'Rec':>6}  {'F1':>6}"
    )
    sep = "─" * len(header)
    print(f"\nVOCE EVAL REPORT  (binary: {args.binary})")
    print("═" * len(header))
    print(header)
    print(sep)

    total = {"tp": 0, "fp": 0, "tn": 0, "fn": 0}
    any_failure = False

    for wav_path in wav_files:
        gt_path = wav_path.with_suffix("").with_suffix(".gt.jsonl")
        name = wav_path.name[:col_name]

        try:
            preds = run_voce_eval(args.binary, wav_path)
            gt = load_gt(gt_path)
            c = match_predictions(preds, gt)
            m = metrics(c)
        except Exception as exc:
            print(f"  ERROR [{wav_path.name}]: {exc}", file=sys.stderr)
            any_failure = True
            continue

        for k in total:
            total[k] += c[k]

        print(
            f"{name:<{col_name}} {c['tp']:>4} {c['fp']:>4} {c['tn']:>4} {c['fn']:>4}"
            f"  {fmt(m['fpr'], '.1%'):>6}  {fmt(m['recall'], '.1%'):>6}  {fmt(m['f1']):>6}"
        )

    # Aggregate row
    tm = metrics(total)
    print(sep)
    print(
        f"{'AGGREGATE':<{col_name}} {total['tp']:>4} {total['fp']:>4}"
        f" {total['tn']:>4} {total['fn']:>4}"
        f"  {fmt(tm['fpr'], '.1%'):>6}  {fmt(tm['recall'], '.1%'):>6}  {fmt(tm['f1']):>6}"
    )
    print("═" * len(header))

    # Pass criteria
    fpr_ok = (not math.isnan(tm["fpr"])) and tm["fpr"] < args.fpr_limit
    rec_ok = (not math.isnan(tm["recall"])) and tm["recall"] > args.recall_min
    print("\nPASS CRITERIA")
    print(
        f"  FPR  < {args.fpr_limit:.0%}:  "
        + ("✅" if fpr_ok else "❌")
        + f"  {fmt(tm['fpr'], '.1%')}"
        + ("" if fpr_ok else f"  — try raising threshold")
    )
    print(
        f"  Recall > {args.recall_min:.0%}:  "
        + ("✅" if rec_ok else "❌")
        + f"  {fmt(tm['recall'], '.1%')}"
        + ("" if rec_ok else "  — try lowering threshold")
    )
    overall_ok = fpr_ok and rec_ok and not any_failure
    print(f"  Overall:  {'✅ PASS' if overall_ok else '❌ FAIL'}\n")

    sys.exit(0 if overall_ok else 1)


if __name__ == "__main__":
    main()
