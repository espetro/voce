#!/usr/bin/env python3
"""
Build test fixtures from the Multi-stream Spontaneous Conversation (MSC) dataset.

Each MSC conversation has two WAV files — one per speaker's microphone (16 kHz mono PCM).
We mix both channels to simulate realistic two-speaker overlap, then use transcript
timestamps to derive per-window ground-truth labels.

Usage:
    python eval/scripts/make_msc_tests.py \\
        --msc-dir "~/Downloads/Multi-stream Spontaneous Conversation Training Dataset" \\
        [--enrolled-speaker ID029] \\
        [--conversation Group0030_S004] \\
        [--num-tests 10] \\
        [--fixtures-dir eval/fixtures]

The script writes fixtures to:
    <fixtures-dir>/msc/alimeeting/tests/
so that eval_audio.py can be pointed at <fixtures-dir>/msc with --fixtures-dir.
"""

import argparse
import json
import pathlib
import re
import sys
from dataclasses import dataclass
from math import gcd
from typing import List, Optional, Tuple

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

# ──────────────────────────────────────────────────────────────────────────────
# Constants — must match the Rust binary and eval_audio.py
# ──────────────────────────────────────────────────────────────────────────────
SAMPLE_RATE = 22050   # Target sample rate written to fixture WAVs
SOURCE_SR   = 16000   # MSC dataset native sample rate
HOP_S       = 0.5     # Analysis hop (matches EmbeddingWindowAccumulator)

# Enrollment criteria
MIN_ENROLLMENT_S    = 15.0   # Minimum total enrollment duration
MAX_ENROLLMENT_S    = 20.0   # Cap to avoid over-long enrollment files
ENROLLMENT_GUARD_S  = 1.5    # No other-speaker speech within this many seconds
MAX_STRETCH_GAP_S   = 2.0    # Max gap between utterances in the same "stretch"

# Test window criteria
WINDOW_DURATION_S    = 30.0  # Fixed test window length
MIN_ENROLLED_SPEECH  = 5.0   # Enrolled speaker must have at least this much speech per window
MIN_OTHER_SPEECH     = 5.0   # Other speaker must have at least this much speech per window

# ──────────────────────────────────────────────────────────────────────────────
# Known conversation → speaker-pair mapping
# ──────────────────────────────────────────────────────────────────────────────
CONVERSATION_PAIRS = {
    ("Group0030_S004", "ID029"): "ID024",
    ("Group0030_S004", "ID024"): "ID029",
    ("Group0046_S009", "ID023"): "ID026",
    ("Group0046_S009", "ID026"): "ID023",
    ("Group0006_S001", "ID164"): "ID165",
    ("Group0006_S001", "ID165"): "ID164",
    ("Group0078_S004", "ID098"): "ID099",
    ("Group0078_S004", "ID099"): "ID098",
    ("Group0078_S005", "ID098"): "ID099",
    ("Group0078_S005", "ID099"): "ID098",
    ("Group0078_S009", "ID098"): "ID099",
    ("Group0078_S009", "ID099"): "ID098",
    ("Group0078_S013", "ID098"): "ID099",
    ("Group0078_S013", "ID099"): "ID098",
    ("Group0078_S015", "ID098"): "ID099",
    ("Group0078_S015", "ID099"): "ID098",
}

# Non-speech noise tags used in MSC transcripts
NOISE_TAGS = frozenset(
    ["[*]", "[NPS]", "[LAUGHTER]", "[SONANT]", "[ENS]", "[MUSIC]", "[SYSTEM]", "[PII]"]
)


# ──────────────────────────────────────────────────────────────────────────────
# Data types
# ──────────────────────────────────────────────────────────────────────────────
@dataclass
class Utterance:
    start: float
    end: float
    speaker: str
    text: str

    @property
    def duration(self) -> float:
        return self.end - self.start


# ──────────────────────────────────────────────────────────────────────────────
# Transcript parsing
# ──────────────────────────────────────────────────────────────────────────────
def parse_msc_transcript(path: pathlib.Path) -> List[Utterance]:
    """Parse an MSC TXT transcript into Utterance objects.

    Format per line (tab-separated):
        [t_start,t_end]  speaker_id  gender  text
    Lines with speaker_id "0" are non-speech events.
    """
    utts: List[Utterance] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line:
            continue
        parts = line.split("\t")
        if len(parts) < 4:
            continue
        m = re.match(r"\[(\d+\.?\d*),(\d+\.?\d*)\]", parts[0])
        if not m:
            continue
        utts.append(
            Utterance(
                start=float(m.group(1)),
                end=float(m.group(2)),
                speaker=parts[1].strip(),
                text=parts[3].strip(),
            )
        )
    return utts


def is_noise(text: str) -> bool:
    return text.strip() in NOISE_TAGS


# ──────────────────────────────────────────────────────────────────────────────
# Audio helpers
# ──────────────────────────────────────────────────────────────────────────────
def load_mono(path: pathlib.Path) -> Tuple[np.ndarray, int]:
    """Load a WAV as mono float32 at its native sample rate."""
    data, sr = sf.read(str(path), dtype="float32", always_2d=True)
    if data.shape[1] > 1:
        data = data.mean(axis=1)
    else:
        data = data.squeeze()
    return data.astype(np.float32), sr


def resample_to_target(data: np.ndarray, orig_sr: int) -> np.ndarray:
    if orig_sr == SAMPLE_RATE:
        return data
    g = gcd(orig_sr, SAMPLE_RATE)
    return resample_poly(data, SAMPLE_RATE // g, orig_sr // g).astype(np.float32)


def peak_normalize(arr: np.ndarray, ceiling: float = 0.99) -> np.ndarray:
    peak = np.abs(arr).max()
    if peak < 1e-6:
        return arr.astype(np.float32)
    if peak > ceiling:
        arr = arr * (ceiling / peak)
    return arr.astype(np.float32)


# ──────────────────────────────────────────────────────────────────────────────
# Enrollment clip selection
# ──────────────────────────────────────────────────────────────────────────────
def find_enrollment_stretches(
    enrolled_utts: List[Utterance],
    other_utts: List[Utterance],
    enrolled_id: str,
    other_id: str,
    exclude_ranges: Optional[List[Tuple[float, float]]] = None,
) -> List[Tuple[float, float]]:
    """Return (start, end) stretches of clean enrolled-speaker solo speech.

    A stretch is a group of enrolled-speaker utterances with gaps ≤ MAX_STRETCH_GAP_S.
    Stretches are excluded if:
    - They overlap any time range in exclude_ranges (test windows).
    - Any other-speaker utterance is within ENROLLMENT_GUARD_S of the stretch boundary.
    """
    speech = [
        u for u in enrolled_utts
        if u.speaker == enrolled_id and not is_noise(u.text)
    ]
    if not speech:
        return []

    # Exclude utterances that fall inside a test window
    if exclude_ranges:
        speech = [
            u for u in speech
            if not any(u.end > s and u.start < e for s, e in exclude_ranges)
        ]
    if not speech:
        return []

    # Group into stretches
    stretches: List[Tuple[float, float]] = []
    ss, se = speech[0].start, speech[0].end
    for u in speech[1:]:
        if u.start - se <= MAX_STRETCH_GAP_S:
            se = max(se, u.end)
        else:
            stretches.append((ss, se))
            ss, se = u.start, u.end
    stretches.append((ss, se))

    # Filter by guard band: no other-speaker utterance within ENROLLMENT_GUARD_S
    other_speech = [u for u in other_utts if u.speaker == other_id]
    clean: List[Tuple[float, float]] = []
    for (s, e) in stretches:
        if e - s < 1.0:
            continue
        nearby = any(
            not (o.end < s - ENROLLMENT_GUARD_S or o.start > e + ENROLLMENT_GUARD_S)
            for o in other_speech
        )
        if not nearby:
            clean.append((s, e))

    # Longest stretches first
    clean.sort(key=lambda x: -(x[1] - x[0]))
    return clean


# ──────────────────────────────────────────────────────────────────────────────
# Test window selection
# ──────────────────────────────────────────────────────────────────────────────
def speech_time_in_window(
    utts: List[Utterance], speaker_id: str, t_start: float, t_end: float
) -> float:
    """Seconds of speech by speaker_id overlapping [t_start, t_end]."""
    total = 0.0
    for u in utts:
        if u.speaker != speaker_id:
            continue
        seg_s = max(u.start, t_start)
        seg_e = min(u.end, t_end)
        if seg_e > seg_s:
            total += seg_e - seg_s
    return total


def find_test_windows(
    enrolled_utts: List[Utterance],
    other_utts: List[Utterance],
    enrolled_id: str,
    other_id: str,
    duration_s: float,
    num_tests: int,
) -> List[dict]:
    """Return up to num_tests non-overlapping 30-second windows where both speakers
    contribute at least MIN_ENROLLED_SPEECH / MIN_OTHER_SPEECH seconds of speech."""
    windows = []
    t = 0.0
    while t + WINDOW_DURATION_S <= duration_s:
        t_end = t + WINDOW_DURATION_S
        enrolled_s = speech_time_in_window(enrolled_utts, enrolled_id, t, t_end)
        other_s    = speech_time_in_window(other_utts,   other_id,   t, t_end)
        if enrolled_s >= MIN_ENROLLED_SPEECH and other_s >= MIN_OTHER_SPEECH:
            windows.append(
                {
                    "t_start": t,
                    "t_end": t_end,
                    "enrolled_speech_s": enrolled_s,
                    "other_speech_s": other_s,
                }
            )
        t += WINDOW_DURATION_S

    return windows[:num_tests]


# ──────────────────────────────────────────────────────────────────────────────
# GT JSONL generation
# ──────────────────────────────────────────────────────────────────────────────
def make_gt_jsonl(
    enrolled_utts: List[Utterance],
    enrolled_id: str,
    t_start_abs: float,
    window_duration: float,
    hop: float = HOP_S,
) -> List[dict]:
    """Build per-window GT labels for a test segment.

    Timestamps are relative to the start of the test WAV (0.0-based).
    A window is labeled is_enrolled=True if the enrolled speaker is speaking
    at the window centre ± half-hop.
    """
    enrolled_speech = [u for u in enrolled_utts if u.speaker == enrolled_id]
    rows = []
    t_rel = 0.0
    while t_rel < window_duration:
        centre_abs = t_start_abs + t_rel + hop / 2
        is_enrolled = any(u.start <= centre_abs < u.end for u in enrolled_speech)
        rows.append({"t_s": round(t_rel, 3), "is_enrolled": is_enrolled})
        t_rel += hop
    return rows


# ──────────────────────────────────────────────────────────────────────────────
# CLI
# ──────────────────────────────────────────────────────────────────────────────
def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "--msc-dir",
        type=pathlib.Path,
        required=True,
        help="Root of the MSC dataset (contains WAV/ and TXT/ subdirectories)",
    )
    p.add_argument(
        "--enrolled-speaker",
        default="ID029",
        help="Speaker ID to enroll (default: ID029)",
    )
    p.add_argument(
        "--conversation",
        default="Group0030_S004",
        help="Conversation identifier, e.g. Group0030_S004 (default: Group0030_S004)",
    )
    p.add_argument(
        "--num-tests",
        type=int,
        default=10,
        help="Maximum number of test windows to generate (default: 10)",
    )
    p.add_argument(
        "--fixtures-dir",
        type=pathlib.Path,
        default=pathlib.Path("eval/fixtures"),
        help="Root fixtures directory (default: eval/fixtures)",
    )
    return p.parse_args()


def main() -> None:
    args = parse_args()
    msc_dir     = args.msc_dir.expanduser()
    enrolled_id = args.enrolled_speaker
    conversation = args.conversation

    # Resolve the other speaker
    key = (conversation, enrolled_id)
    if key not in CONVERSATION_PAIRS:
        print(
            f"ERROR: Unknown conversation/speaker pair: {key}\n"
            f"Known: {sorted(CONVERSATION_PAIRS.keys())}",
            file=sys.stderr,
        )
        sys.exit(1)
    other_id = CONVERSATION_PAIRS[key]

    # Output directory — named alimeeting/tests so eval_audio.py can find it
    # when called with --fixtures-dir <fixtures-dir>/msc
    tests_dir = args.fixtures_dir / "msc" / "alimeeting" / "tests"
    tests_dir.mkdir(parents=True, exist_ok=True)

    # Locate source files
    wav_dir = msc_dir / "WAV"
    txt_dir = msc_dir / "TXT"

    enrolled_wav = wav_dir / f"{conversation}_0_{enrolled_id}.wav"
    other_wav    = wav_dir / f"{conversation}_0_{other_id}.wav"
    enrolled_txt = txt_dir / f"{conversation}_0_{enrolled_id}.txt"
    other_txt    = txt_dir / f"{conversation}_0_{other_id}.txt"

    for path in (enrolled_wav, other_wav, enrolled_txt, other_txt):
        if not path.exists():
            print(f"ERROR: Missing file: {path}", file=sys.stderr)
            sys.exit(1)

    print(f"Enrolled: {enrolled_id}  |  Other: {other_id}  |  Conversation: {conversation}")
    print()

    # ── Load & resample ───────────────────────────────────────────────────────
    print("Loading audio...")
    enrolled_raw, src_sr = load_mono(enrolled_wav)
    other_raw, _         = load_mono(other_wav)
    print(f"  Source rate : {src_sr} Hz")
    print(f"  Enrolled ch : {len(enrolled_raw)/src_sr:.1f}s")
    print(f"  Other ch    : {len(other_raw)/src_sr:.1f}s")

    print("Resampling to 22050 Hz...")
    enrolled_audio = resample_to_target(enrolled_raw, src_sr)
    other_audio    = resample_to_target(other_raw, src_sr)
    del enrolled_raw, other_raw

    n = min(len(enrolled_audio), len(other_audio))
    enrolled_audio = enrolled_audio[:n]
    other_audio    = other_audio[:n]
    duration_s = n / SAMPLE_RATE
    print(f"  After trim  : {duration_s:.1f}s at {SAMPLE_RATE} Hz")
    print()

    # ── Parse transcripts ─────────────────────────────────────────────────────
    print("Parsing transcripts...")
    enrolled_utts = parse_msc_transcript(enrolled_txt)
    other_utts    = parse_msc_transcript(other_txt)

    enrolled_speech_utts = [u for u in enrolled_utts if u.speaker == enrolled_id and not is_noise(u.text)]
    other_speech_utts    = [u for u in other_utts    if u.speaker == other_id    and not is_noise(u.text)]
    print(
        f"  {enrolled_id}: {len(enrolled_speech_utts)} utterances, "
        f"{sum(u.duration for u in enrolled_speech_utts):.1f}s"
    )
    print(
        f"  {other_id}: {len(other_speech_utts)} utterances, "
        f"{sum(u.duration for u in other_speech_utts):.1f}s"
    )
    print()

    # ── Find test windows first (so enrollment can avoid them) ────────────────
    print("Finding test windows...")
    windows = find_test_windows(
        enrolled_utts, other_utts, enrolled_id, other_id, duration_s, args.num_tests
    )
    print(
        f"  {len(windows)} qualifying windows "
        f"(≥{MIN_ENROLLED_SPEECH:.0f}s enrolled, ≥{MIN_OTHER_SPEECH:.0f}s other)"
    )
    if not windows:
        print("ERROR: No qualifying test windows found.", file=sys.stderr)
        sys.exit(1)
    exclude_ranges = [(w["t_start"], w["t_end"]) for w in windows]
    print()

    # ── Find enrollment clips (outside all test windows) ─────────────────────
    print("Finding enrollment clips...")
    stretches = find_enrollment_stretches(
        enrolled_utts, other_utts, enrolled_id, other_id, exclude_ranges
    )
    if not stretches:
        print(
            "WARNING: No clean enrollment stretches found outside test windows; "
            "retrying without exclusion...",
            file=sys.stderr,
        )
        stretches = find_enrollment_stretches(
            enrolled_utts, other_utts, enrolled_id, other_id
        )
    if not stretches:
        print("ERROR: No enrollment stretches found at all.", file=sys.stderr)
        sys.exit(1)

    # Concatenate stretches to reach MIN_ENROLLMENT_S
    enroll_clips: List[Tuple[float, float]] = []
    total_s = 0.0
    for (s, e) in stretches:
        if total_s >= MIN_ENROLLMENT_S:
            break
        remaining = MAX_ENROLLMENT_S - total_s
        clip_end = min(e, s + remaining)
        enroll_clips.append((s, clip_end))
        total_s += clip_end - s

    print(f"  {len(enroll_clips)} stretch(es), {total_s:.1f}s total")

    # Extract and concatenate (already at 22050 Hz)
    parts = []
    for (s, e) in enroll_clips:
        parts.append(enrolled_audio[int(s * SAMPLE_RATE) : int(e * SAMPLE_RATE)])
    enroll_audio = np.concatenate(parts)
    enroll_audio = peak_normalize(enroll_audio)

    enroll_filename = f"msc_enroll_{enrolled_id}.wav"
    sf.write(str(tests_dir / enroll_filename), enroll_audio, SAMPLE_RATE, subtype="FLOAT")
    print(f"  Written: {enroll_filename}")
    print()

    # ── Generate test cases ───────────────────────────────────────────────────
    test_cases = []
    for i, win in enumerate(windows, start=1):
        test_id = f"msc_{i:03d}"
        t_start = win["t_start"]
        t_end   = win["t_end"]
        print(
            f"Generating {test_id}  ({t_start:.0f}s–{t_end:.0f}s  "
            f"enrolled={win['enrolled_speech_s']:.1f}s  other={win['other_speech_s']:.1f}s)"
        )

        # Slice both channels
        s_idx = int(t_start * SAMPLE_RATE)
        e_idx = int(t_end   * SAMPLE_RATE)
        e_slice = enrolled_audio[s_idx:e_idx]  # note: enrolled_audio is full-file now
        # Re-read from the full other_audio
        o_slice = other_audio[s_idx:e_idx]
        nn = min(len(e_slice), len(o_slice))

        # Mix 1:1 then peak-normalize
        mixed = enrolled_audio[s_idx : s_idx + nn] + other_audio[s_idx : s_idx + nn]
        mixed = peak_normalize(mixed)

        # Write test WAV
        test_wav_name = f"{test_id}.wav"
        sf.write(str(tests_dir / test_wav_name), mixed, SAMPLE_RATE, subtype="FLOAT")

        # GT JSONL (relative timestamps)
        gt_rows = make_gt_jsonl(enrolled_utts, enrolled_id, t_start, WINDOW_DURATION_S)
        gt_name = f"{test_id}.gt.jsonl"
        with open(tests_dir / gt_name, "w") as f:
            for row in gt_rows:
                f.write(json.dumps(row) + "\n")

        enrolled_windows = sum(1 for r in gt_rows if r["is_enrolled"])
        print(
            f"  {test_wav_name}  GT: {enrolled_windows}/{len(gt_rows)} windows labeled enrolled"
        )

        test_cases.append(
            {
                "test_id": test_id,
                "enrolled_speaker": enrolled_id,
                "enrollment_file": enroll_filename,
                "test_wav": test_wav_name,
                "gt_jsonl": gt_name,
                "source_conversation": conversation,
                "segment_start_s": t_start,
                "segment_end_s": t_end,
                "enrolled_speech_s": round(win["enrolled_speech_s"], 2),
                "other_speech_s": round(win["other_speech_s"], 2),
            }
        )

    # ── Write manifest ────────────────────────────────────────────────────────
    manifest_path = tests_dir / "manifest.json"
    with open(manifest_path, "w") as f:
        json.dump({"test_cases": test_cases}, f, indent=2)

    print()
    print("=" * 60)
    print(f"Generated {len(test_cases)} test cases")
    print(f"Enrollment  : {enroll_filename} ({total_s:.1f}s)")
    print(f"Manifest    : {manifest_path}")
    print(f"Output dir  : {tests_dir}")
    print("=" * 60)
    print()
    print("Next steps:")
    print("  cargo build --release")
    print("  make -f eval/Makefile eval-msc-run")


if __name__ == "__main__":
    main()
