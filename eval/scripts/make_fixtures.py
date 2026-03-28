#!/usr/bin/env python3
"""
Generate mixed WAV fixtures and ground-truth JSONL sidecars for Voce eval.

Requires:
  eval/fixtures/enrolled.wav  — your voice recording at any sample rate/bit depth

Reads other-speaker WAVs from eval/fixtures/other/spk_*.wav (written by
download_samples.py) and produces:

  eval/fixtures/pure_enrolled.wav            — 30 s of enrolled speaker only
  eval/fixtures/pure_enrolled.gt.jsonl
  eval/fixtures/pure_other_{id}.wav          — 30 s of each other speaker
  eval/fixtures/pure_other_{id}.gt.jsonl
  eval/fixtures/mixed_spk{id}_snr{snr}.wav   — 60 s alternating 3-s segments
  eval/fixtures/mixed_spk{id}_snr{snr}.gt.jsonl

The SNR values tested are 0 dB (equal loudness) and -5 dB (other speaker louder).

Ground-truth JSONL format — one line per 0.5-s hop window:
  {"t_s": 0.0, "is_enrolled": true}

Usage:
    python eval/scripts/make_fixtures.py [--fixtures-dir DIR] [--snrs 0 -5] [--no-pure]
"""
import argparse
import json
import pathlib
import sys
from math import gcd

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

TARGET_SR = 22050
HOP = 11025       # 0.5-s hop — must match EmbeddingWindowAccumulator.hop_size
WINDOW = 22050    # 1-s window — must match EmbeddingWindowAccumulator.window_size
SEG_LEN_S = 3.0
DEFAULT_SNRS = [0, -5]
DEFAULT_FIXTURES = pathlib.Path("eval/fixtures")


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--fixtures-dir", type=pathlib.Path, default=DEFAULT_FIXTURES,
                   help=f"Root fixtures directory (default: {DEFAULT_FIXTURES})")
    p.add_argument("--snrs", type=int, nargs="+", default=DEFAULT_SNRS,
                   help=f"SNR values in dB to generate (default: {DEFAULT_SNRS})")
    p.add_argument("--no-pure", action="store_true",
                   help="Skip writing pure_enrolled and pure_other files")
    return p.parse_args()


def load_and_resample(path: pathlib.Path, target_sr: int) -> np.ndarray:
    """Load WAV/FLAC, collapse to mono, resample to target_sr using polyphase filter."""
    data, sr = sf.read(str(path), dtype="float32", always_2d=False)
    if data.ndim > 1:
        data = data.mean(axis=1)
    if sr != target_sr:
        g = gcd(sr, target_sr)
        data = resample_poly(data, target_sr // g, sr // g).astype(np.float32)
    return data


def tile_to(arr: np.ndarray, n: int) -> np.ndarray:
    """Repeat arr until at least n samples, then trim to exactly n."""
    if len(arr) == 0:
        return np.zeros(n, dtype=np.float32)
    reps = (n + len(arr) - 1) // len(arr)
    return np.tile(arr, reps)[:n].astype(np.float32)


def scale_to_snr(signal: np.ndarray, noise: np.ndarray, snr_db: float) -> np.ndarray:
    """
    Scale noise so that its RMS is snr_db below signal's RMS, then return signal + noise.
    Peak-normalises to ±0.99 to avoid clipping.
    """
    sig_rms = np.sqrt(np.mean(signal ** 2)) + 1e-9
    noise_rms = np.sqrt(np.mean(noise ** 2)) + 1e-9
    # target noise RMS = sig_rms / 10^(snr_db/20)
    target_noise_rms = sig_rms / (10.0 ** (snr_db / 20.0))
    scaled_noise = noise * (target_noise_rms / noise_rms)
    mixed = signal + scaled_noise
    peak = np.abs(mixed).max()
    if peak > 0.99:
        mixed = mixed * (0.99 / peak)
    return mixed.astype(np.float32)


def make_gt_jsonl(
    total_samples: int,
    seg_samples: int,
    first_is_enrolled: bool,
    hop: int,
    sr: int,
) -> list[dict]:
    """
    Build per-window ground truth.  A window's label is determined by which
    3-s segment its centre sample falls in.

    Returns a list of {"t_s": float, "is_enrolled": bool} dicts, one per
    0.5-s hop window (matching EmbeddingWindowAccumulator output timing).
    """
    rows = []
    # Number of windows matches the accumulator: first window after WINDOW samples,
    # then every HOP samples.
    n_windows = max(0, (total_samples - WINDOW) // hop + 1)
    for idx in range(n_windows):
        t_s = round(idx * hop / sr, 3)
        # Centre sample of this window
        centre = idx * hop + WINDOW // 2
        seg_idx = int(centre // seg_samples)
        is_enrolled = (seg_idx % 2 == 0) == first_is_enrolled
        rows.append({"t_s": t_s, "is_enrolled": bool(is_enrolled)})
    return rows


def write_jsonl(path: pathlib.Path, rows: list[dict]) -> None:
    with open(path, "w") as f:
        for row in rows:
            f.write(json.dumps(row) + "\n")


def write_wav_and_gt(
    wav_path: pathlib.Path,
    samples: np.ndarray,
    gt_rows: list[dict],
) -> None:
    sf.write(str(wav_path), samples, TARGET_SR, subtype="FLOAT")
    gt_path = wav_path.with_suffix("").with_suffix(".gt.jsonl")
    write_jsonl(gt_path, gt_rows)
    duration_s = len(samples) / TARGET_SR
    print(f"  Wrote {wav_path.name}  ({duration_s:.1f}s, {len(gt_rows)} windows)")


def main() -> None:
    args = parse_args()
    fixtures: pathlib.Path = args.fixtures_dir

    enrolled_path = fixtures / "enrolled.wav"
    if not enrolled_path.exists():
        print(
            f"ERROR: {enrolled_path} not found.\n"
            "Record your own voice and save it there, then re-run this script.",
            file=sys.stderr,
        )
        sys.exit(1)

    # Discover other-speaker WAVs
    other_wavs = sorted(fixtures.glob("other/spk_*.wav"))
    if not other_wavs:
        print(
            "ERROR: No other-speaker WAVs found in eval/fixtures/other/.\n"
            "Run eval/scripts/download_samples.py first.",
            file=sys.stderr,
        )
        sys.exit(1)

    print(f"Loading enrolled speaker from {enrolled_path}…")
    enrolled_raw = load_and_resample(enrolled_path, TARGET_SR)
    print(f"  {len(enrolled_raw)/TARGET_SR:.1f}s at {TARGET_SR} Hz")

    seg_samples = int(SEG_LEN_S * TARGET_SR)

    # --- Pure enrolled baseline (30 s) ---
    if not args.no_pure:
        total_pure = TARGET_SR * 30
        pure_enrolled = tile_to(enrolled_raw, total_pure)
        gt = make_gt_jsonl(total_pure, seg_samples, first_is_enrolled=True, hop=HOP, sr=TARGET_SR)
        # All windows enrolled
        gt = [{"t_s": r["t_s"], "is_enrolled": True} for r in gt]
        write_wav_and_gt(fixtures / "pure_enrolled.wav", pure_enrolled, gt)

    # --- Per other speaker ---
    for other_path in other_wavs:
        spk_id = other_path.stem  # e.g. "spk_84"
        print(f"\nProcessing {spk_id}…")
        other_raw = load_and_resample(other_path, TARGET_SR)
        print(f"  {len(other_raw)/TARGET_SR:.1f}s at {TARGET_SR} Hz")

        # Pure other baseline (30 s)
        if not args.no_pure:
            total_pure = TARGET_SR * 30
            pure_other = tile_to(other_raw, total_pure)
            gt = [{"t_s": round(i * HOP / TARGET_SR, 3), "is_enrolled": False}
                  for i in range(max(0, (total_pure - WINDOW) // HOP + 1))]
            write_wav_and_gt(fixtures / f"pure_{spk_id}.wav", pure_other, gt)

        # Mixed files at each SNR
        total_mixed = TARGET_SR * 60
        enrolled_tiled = tile_to(enrolled_raw, total_mixed)
        other_tiled = tile_to(other_raw, total_mixed)

        # Build alternating foreground signal: enrolled/other/enrolled/other …
        signal = np.zeros(total_mixed, dtype=np.float32)
        noise = np.zeros(total_mixed, dtype=np.float32)
        for seg_start in range(0, total_mixed, seg_samples):
            seg_end = min(seg_start + seg_samples, total_mixed)
            seg_idx = seg_start // seg_samples
            if seg_idx % 2 == 0:
                # Enrolled speaker is foreground; other is background noise
                signal[seg_start:seg_end] = enrolled_tiled[seg_start:seg_end]
                noise[seg_start:seg_end] = other_tiled[seg_start:seg_end]
            else:
                # Other speaker is foreground; enrolled is background noise
                signal[seg_start:seg_end] = other_tiled[seg_start:seg_end]
                noise[seg_start:seg_end] = enrolled_tiled[seg_start:seg_end]

        for snr in args.snrs:
            mixed = scale_to_snr(signal, noise, float(snr))
            snr_tag = f"{snr:+d}" if snr != 0 else "+0"
            wav_name = f"mixed_{spk_id}_snr{snr_tag}.wav"
            gt = make_gt_jsonl(total_mixed, seg_samples, first_is_enrolled=True, hop=HOP, sr=TARGET_SR)
            write_wav_and_gt(fixtures / wav_name, mixed, gt)

    print("\nAll fixtures written.")


if __name__ == "__main__":
    main()
