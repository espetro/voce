#!/usr/bin/env python3
"""
Build test fixtures from AliMeeting real overlapping speech data.

Replaces the old CMU Arctic artificial concatenation with naturally occurring
overlapping speech from the AliMeeting dataset.

Usage:
    python eval/scripts/make_audio_tests.py [--fixtures-dir DIR]
"""

import argparse
import json
import pathlib
import sys
from math import gcd
from typing import Dict, List, Tuple, Optional

import numpy as np
import soundfile as sf
from scipy.signal import resample_poly

# Import parse_textgrid module
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from parse_textgrid import (
    parse_textgrid,
    extract_speaker_intervals,
    find_overlaps,
    calculate_solo_time,
    find_solo_segments,
    Interval,
)

# Constants (must match eval_audio.py)
SAMPLE_RATE = 22050  # Target sample rate
HOP = 11025  # 0.5s hop (must match EmbeddingWindowAccumulator)
WINDOW = 22050  # 1s analysis window
SOURCE_SR = 16000  # AliMeeting source sample rate

# Test segment criteria
MIN_SEGMENT_S = 10  # Minimum test segment duration
MAX_SEGMENT_S = 30  # Maximum test segment duration
MIN_ENROLLMENT_S = 10  # Minimum enrollment duration
MAX_ENROLLMENT_S = 20  # Maximum enrollment duration
MIN_SOLO_TIME_S = 5  # Minimum solo time per speaker for enrollment
MIN_OVERLAP_S = 1.0  # Minimum overlap duration to consider

DEFAULT_FIXTURES = pathlib.Path("eval/fixtures")


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    p.add_argument(
        "--fixtures-dir",
        type=pathlib.Path,
        default=DEFAULT_FIXTURES,
        help=f"Root fixtures directory (default: {DEFAULT_FIXTURES})",
    )
    p.add_argument(
        "--min-segments",
        type=int,
        default=10,
        help="Minimum number of test segments to generate (default: 10)",
    )
    return p.parse_args()


def load_and_mixdown(path: pathlib.Path) -> np.ndarray:
    """Load multi-channel WAV, mix to mono, return at SOURCE_SR."""
    data, sr = sf.read(str(path), dtype="float32", always_2d=True)
    # Mix to mono by averaging channels
    if data.ndim > 1 and data.shape[1] > 1:
        data = data.mean(axis=1)
    else:
        data = data.squeeze()
    return data.astype(np.float32), sr


def resample_to_target(data: np.ndarray, orig_sr: int) -> np.ndarray:
    """Resample audio to TARGET_SR using high-quality polyphase resampling."""
    if orig_sr == SAMPLE_RATE:
        return data
    g = gcd(orig_sr, SAMPLE_RATE)
    return resample_poly(data, SAMPLE_RATE // g, orig_sr // g).astype(np.float32)


def extract_segment(
    audio_data: np.ndarray,
    orig_sr: int,
    start_s: float,
    end_s: float,
) -> np.ndarray:
    """Extract a time segment from audio and resample to target rate."""
    start_sample = int(start_s * orig_sr)
    end_sample = int(end_s * orig_sr)
    segment = audio_data[start_sample:end_sample]
    return resample_to_target(segment, orig_sr)


def extract_concatenated_segments(
    audio_data: np.ndarray,
    orig_sr: int,
    intervals: List[Interval],
    target_duration: float,
) -> np.ndarray:
    """Extract and concatenate intervals up to target duration."""
    segments = []
    current_duration = 0.0

    for interval in intervals:
        if current_duration >= target_duration:
            break

        start_sample = int(interval.start * orig_sr)
        end_sample = int(interval.end * orig_sr)
        segment = audio_data[start_sample:end_sample]
        segments.append(segment)
        current_duration += interval.duration

    if not segments:
        return np.array([], dtype=np.float32)

    concatenated = np.concatenate(segments)

    # Truncate to exact target duration if exceeded
    target_samples = int(target_duration * orig_sr)
    if len(concatenated) > target_samples:
        concatenated = concatenated[:target_samples]

    return resample_to_target(concatenated, orig_sr)


def peak_normalize(arr: np.ndarray, ceiling: float = 0.99) -> np.ndarray:
    """Peak normalize audio to specified ceiling."""
    peak = np.abs(arr).max()
    if peak > ceiling:
        arr = arr * (ceiling / peak)
    return arr.astype(np.float32)


def make_gt_jsonl(
    enrolled_speaker: str,
    speaker_intervals: Dict[str, List[Interval]],
    segment_start: float,
    segment_end: float,
    hop: float = 0.5,
) -> List[Dict]:
    """
    Build per-window GT for a test segment.

    A window is labeled as enrolled if the enrolled speaker has a non-empty
    interval covering the window center.
    """
    rows = []
    window_s = hop  # Window hop size in seconds

    t = segment_start
    while t < segment_end:
        window_center = t + window_s / 2

        # Check if enrolled speaker is speaking at window center
        is_enrolled = False
        for interval in speaker_intervals.get(enrolled_speaker, []):
            if interval.start <= window_center < interval.end:
                is_enrolled = True
                break

        rows.append(
            {
                "t_s": round(t, 3),
                "is_enrolled": is_enrolled,
            }
        )

        t += hop

    return rows


def find_candidate_segments(
    textgrid_path: pathlib.Path,
    min_duration: float = MIN_SEGMENT_S,
    max_duration: float = MAX_SEGMENT_S,
) -> List[Dict]:
    """
    Find candidate test segments in a TextGrid file.

    Returns segments with:
    - At least 2 speakers overlapping
    - Duration between min_duration and max_duration
    - All speakers have at least MIN_SOLO_TIME_S solo time available
    """
    textgrid = parse_textgrid(str(textgrid_path))

    # Extract speaker intervals
    speaker_intervals = {}
    speaker_names = []

    for tier in textgrid:
        speaker_id = tier.name
        speaker_names.append(speaker_id)
        intervals = extract_speaker_intervals(tier)
        if intervals:
            speaker_intervals[speaker_id] = intervals

    if len(speaker_intervals) < 2:
        return []

    # Find all overlaps in the file
    max_time = float(textgrid.maxTime)
    overlaps = find_overlaps(speaker_intervals, 0, max_time)

    candidates = []

    # Generate candidate windows around overlap regions
    for overlap in overlaps:
        if overlap.duration < MIN_OVERLAP_S:
            continue

        # Try different window sizes and positions
        for duration in [15.0, 20.0, 25.0, 30.0]:
            if duration < min_duration or duration > max_duration:
                continue

            # Center window on overlap
            window_start = overlap.start - (duration - overlap.duration) / 2
            window_start = max(0, window_start)
            window_end = min(window_start + duration, max_time)
            window_start = window_end - duration  # Adjust to maintain duration
            window_start = max(0, window_start)

            if window_end - window_start < min_duration:
                continue

            # Check if all speakers have sufficient solo time outside window
            all_have_solo = True
            solo_times = {}

            for speaker_id, intervals in speaker_intervals.items():
                # Calculate solo time outside the test window
                solo_time = 0.0
                for interval in intervals:
                    # Interval outside window
                    if interval.end <= window_start or interval.start >= window_end:
                        solo_time += interval.duration
                    # Partial overlap - take portion outside
                    elif interval.start < window_start:
                        solo_time += window_start - interval.start
                    elif interval.end > window_end:
                        solo_time += interval.end - window_end

                solo_times[speaker_id] = solo_time
                if solo_time < MIN_SOLO_TIME_S:
                    all_have_solo = False
                    break

            if all_have_solo:
                candidates.append(
                    {
                        "source_file": textgrid_path.stem,
                        "start": window_start,
                        "end": window_end,
                        "duration": window_end - window_start,
                        "num_speakers": len(speaker_intervals),
                        "overlap_duration": overlap.duration,
                        "speakers": list(speaker_intervals.keys()),
                        "solo_times": solo_times,
                        "speaker_intervals": speaker_intervals,
                    }
                )

    # Sort by number of speakers (prefer more), then by overlap duration
    candidates.sort(key=lambda x: (-x["num_speakers"], -x["overlap_duration"]))

    # Remove overlapping candidates (don't reuse same time regions)
    filtered = []
    used_regions = []

    for candidate in candidates:
        overlap_with_used = False
        for used in used_regions:
            # Check if this candidate overlaps significantly with used region
            overlap_start = max(candidate["start"], used[0])
            overlap_end = min(candidate["end"], used[1])
            if overlap_end > overlap_start:
                overlap_duration = overlap_end - overlap_start
                if overlap_duration > candidate["duration"] * 0.5:
                    overlap_with_used = True
                    break

        if not overlap_with_used:
            filtered.append(candidate)
            used_regions.append((candidate["start"], candidate["end"]))

    return filtered


def generate_test_case(
    candidate: Dict,
    audio_path: pathlib.Path,
    output_dir: pathlib.Path,
    test_id: str,
    enrolled_speaker: str,
) -> Optional[Dict]:
    """Generate a single test case from a candidate segment."""

    # Load audio
    audio_data, orig_sr = load_and_mixdown(audio_path)

    # Extract test segment
    segment_start = candidate["start"]
    segment_end = candidate["end"]
    test_audio = extract_segment(audio_data, orig_sr, segment_start, segment_end)
    test_audio = peak_normalize(test_audio)

    # Write test WAV
    test_wav_path = output_dir / f"{test_id}.wav"
    sf.write(str(test_wav_path), test_audio, SAMPLE_RATE, subtype="FLOAT")

    # Extract enrollment audio (solo segments outside test window)
    speaker_intervals = candidate["speaker_intervals"]
    enrolled_intervals = speaker_intervals.get(enrolled_speaker, [])

    # Find solo segments outside the test window
    solo_segments = []
    for interval in enrolled_intervals:
        # Interval completely before test window
        if interval.end <= segment_start:
            solo_segments.append(interval)
        # Interval completely after test window
        elif interval.start >= segment_end:
            solo_segments.append(interval)
        # Partial overlap - take portion outside
        elif interval.start < segment_start:
            solo_segments.append(Interval(interval.start, segment_start, interval.text))
        elif interval.end > segment_end:
            solo_segments.append(Interval(segment_end, interval.end, interval.text))

    # Sort by duration (prefer longer segments)
    solo_segments.sort(key=lambda x: -x.duration)

    # Extract enrollment audio (10-20s)
    enrollment_audio = extract_concatenated_segments(
        audio_data, orig_sr, solo_segments, MAX_ENROLLMENT_S
    )

    if len(enrollment_audio) < MIN_ENROLLMENT_S * SAMPLE_RATE:
        print(
            f"  WARNING: {test_id} enrollment too short ({len(enrollment_audio) / SAMPLE_RATE:.1f}s)"
        )
        # Continue anyway, but note the issue

    enrollment_audio = peak_normalize(enrollment_audio)

    # Write enrollment WAV
    speaker_suffix = enrolled_speaker.replace("N_SPK", "")
    enroll_wav_path = output_dir / f"{test_id}_enroll_{speaker_suffix}.wav"
    sf.write(str(enroll_wav_path), enrollment_audio, SAMPLE_RATE, subtype="FLOAT")

    # Generate GT JSONL
    gt_rows = make_gt_jsonl(
        enrolled_speaker,
        speaker_intervals,
        segment_start,
        segment_end,
    )
    gt_path = output_dir / f"{test_id}.gt.jsonl"
    with open(gt_path, "w") as f:
        for row in gt_rows:
            f.write(json.dumps(row) + "\n")

    return {
        "test_id": test_id,
        "source_file": candidate["source_file"],
        "segment_start": segment_start,
        "segment_end": segment_end,
        "num_speakers": candidate["num_speakers"],
        "enrolled_speaker": enrolled_speaker,
        "enrollment_file": enroll_wav_path.name,
        "test_wav": test_wav_path.name,
        "gt_jsonl": gt_path.name,
        "overlap_duration_s": round(candidate["overlap_duration"], 2),
        "solo_time_per_speaker": {
            k: round(v, 2) for k, v in candidate["solo_times"].items()
        },
    }


def match_audio_to_textgrid(
    textgrid_path: pathlib.Path, audio_dir: pathlib.Path
) -> Optional[pathlib.Path]:
    """Find matching audio file for a TextGrid file."""
    base_name = textgrid_path.stem  # e.g., "R8001_M8004"

    # Look for audio file with pattern R8001_M8004_MS*.wav
    for audio_file in audio_dir.glob(f"{base_name}_MS*.wav"):
        return audio_file

    return None


def main() -> None:
    args = parse_args()
    fixtures_dir = args.fixtures_dir

    alimeeting_dir = fixtures_dir / "alimeeting"
    audio_dir = alimeeting_dir / "audio"
    textgrid_dir = alimeeting_dir / "textgrid"
    tests_dir = alimeeting_dir / "tests"

    # Check directories exist
    if not alimeeting_dir.exists():
        print(
            f"ERROR: {alimeeting_dir} not found. Run download_alimeeting.py first.",
            file=sys.stderr,
        )
        sys.exit(1)

    if not audio_dir.exists() or not textgrid_dir.exists():
        print(
            f"ERROR: Missing audio/ or textgrid/ subdirectories in {alimeeting_dir}",
            file=sys.stderr,
        )
        sys.exit(1)

    # Create tests directory
    tests_dir.mkdir(parents=True, exist_ok=True)

    print("Scanning AliMeeting dataset...")

    # Find all TextGrid files
    textgrid_files = sorted(textgrid_dir.glob("*.TextGrid"))

    if not textgrid_files:
        print(f"ERROR: No TextGrid files found in {textgrid_dir}", file=sys.stderr)
        sys.exit(1)

    print(f"Found {len(textgrid_files)} TextGrid files")

    # Find candidate segments from all files
    all_candidates = []

    for tg_file in textgrid_files:
        print(f"  Analyzing {tg_file.name}...")
        candidates = find_candidate_segments(tg_file)
        print(f"    Found {len(candidates)} candidate segments")
        all_candidates.extend(candidates)

    print(f"\nTotal candidates: {len(all_candidates)}")

    if len(all_candidates) < args.min_segments:
        print(
            f"WARNING: Found only {len(all_candidates)} candidates, need {args.min_segments}",
            file=sys.stderr,
        )

    # Generate test cases
    test_cases = []
    test_id = 1

    # Track how many of each speaker count we have
    counts_by_speakers = {2: 0, 3: 0, 4: 0}

    for candidate in all_candidates:
        if test_id > args.min_segments * 2:  # Limit total tests
            break

        # Generate one test per speaker in the segment (rotate enrolled)
        speakers = candidate["speakers"]
        num_speakers = len(speakers)

        # Find matching audio file
        audio_path = match_audio_to_textgrid(
            textgrid_dir / f"{candidate['source_file']}.TextGrid", audio_dir
        )

        if not audio_path:
            print(f"  WARNING: No matching audio for {candidate['source_file']}")
            continue

        # Generate test with each speaker as enrolled (up to 2 per segment to manage total)
        for enrolled_speaker in speakers[:2]:
            test_id_str = f"ali_{test_id:03d}"

            print(f"\nGenerating {test_id_str}...")
            print(f"  Source: {candidate['source_file']}")
            print(f"  Time: {candidate['start']:.1f}s - {candidate['end']:.1f}s")
            print(f"  Speakers: {num_speakers}, Enrolled: {enrolled_speaker}")

            try:
                test_case = generate_test_case(
                    candidate,
                    audio_path,
                    tests_dir,
                    test_id_str,
                    enrolled_speaker,
                )

                if test_case:
                    test_cases.append(test_case)
                    counts_by_speakers[num_speakers] += 1
                    test_id += 1

                    print(
                        f"  Created: {test_case['test_wav']}, {test_case['enrollment_file']}"
                    )

            except Exception as e:
                print(
                    f"  ERROR: Failed to generate {test_id_str}: {e}", file=sys.stderr
                )
                continue

    # Generate manifest
    manifest = {"test_cases": test_cases}
    manifest_path = tests_dir / "manifest.json"

    with open(manifest_path, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"\n{'=' * 60}")
    print(f"Generated {len(test_cases)} test cases:")
    print(f"  2-speaker: {counts_by_speakers[2]}")
    print(f"  3-speaker: {counts_by_speakers[3]}")
    print(f"  4-speaker: {counts_by_speakers[4]}")
    print(f"\nManifest: {manifest_path}")
    print(f"Output directory: {tests_dir}")
    print(f"{'=' * 60}")


if __name__ == "__main__":
    main()
