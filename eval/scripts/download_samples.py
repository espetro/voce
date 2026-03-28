#!/usr/bin/env python3
"""
Download mini-LibriSpeech (dev-clean-2) and concatenate per-speaker WAVs.

Downloads https://www.openslr.org/resources/31/dev-clean-2.tar.gz once,
caches it locally, then extracts FLACs for the first --num-speakers speakers
(sorted by speaker ID) and writes one concatenated mono WAV per speaker to
eval/fixtures/other/spk_{id}.wav at 16000 Hz 16-bit PCM.

Idempotent: skips speakers whose output file already exists and is non-empty.

Usage:
    python eval/scripts/download_samples.py [--num-speakers N] [--fixtures-dir DIR]
"""
import argparse
import io
import pathlib
import sys
import tarfile

import requests
import soundfile as sf
import numpy as np
from tqdm import tqdm

MINI_LIBRI_URL = "https://www.openslr.org/resources/31/dev-clean-2.tar.gz"
DEFAULT_FIXTURES = pathlib.Path("eval/fixtures")
DEFAULT_NUM_SPEAKERS = 6


def parse_args() -> argparse.Namespace:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--num-speakers", type=int, default=DEFAULT_NUM_SPEAKERS,
                   help=f"Number of speakers to extract (default: {DEFAULT_NUM_SPEAKERS})")
    p.add_argument("--fixtures-dir", type=pathlib.Path, default=DEFAULT_FIXTURES,
                   help=f"Root fixtures directory (default: {DEFAULT_FIXTURES})")
    return p.parse_args()


def download_file(url: str, dest: pathlib.Path) -> None:
    """Stream-download url to dest with a tqdm progress bar."""
    dest.parent.mkdir(parents=True, exist_ok=True)
    r = requests.get(url, stream=True, timeout=120)
    r.raise_for_status()
    total = int(r.headers.get("content-length", 0))
    with open(dest, "wb") as f, tqdm(
        desc=f"Downloading {dest.name}", total=total, unit="B", unit_scale=True
    ) as bar:
        for chunk in r.iter_content(chunk_size=1 << 20):
            f.write(chunk)
            bar.update(len(chunk))


def discover_speakers(tar_path: pathlib.Path) -> list[int]:
    """Return a sorted list of integer speaker IDs found in the archive."""
    speaker_ids: set[int] = set()
    with tarfile.open(tar_path, "r:gz") as tf:
        for member in tf.getmembers():
            parts = pathlib.PurePosixPath(member.name).parts
            # Structure: LibriSpeech/dev-clean-2/<speaker_id>/...
            if len(parts) >= 3 and parts[1] == "dev-clean-2":
                try:
                    speaker_ids.add(int(parts[2]))
                except ValueError:
                    pass
    return sorted(speaker_ids)


def extract_speaker(tar_path: pathlib.Path, speaker_id: int) -> np.ndarray:
    """
    Extract all FLACs for speaker_id from the archive, concatenate, and return
    as a mono float32 array at 16000 Hz.
    """
    prefix = f"LibriSpeech/dev-clean-2/{speaker_id}/"
    all_samples: list[np.ndarray] = []

    with tarfile.open(tar_path, "r:gz") as tf:
        members = sorted(
            [m for m in tf.getmembers() if m.name.startswith(prefix) and m.name.endswith(".flac")],
            key=lambda m: m.name,
        )
        if not members:
            raise RuntimeError(f"No FLAC files found for speaker {speaker_id} in archive")

        for m in tqdm(members, desc=f"  Speaker {speaker_id}", unit="file", leave=False):
            fobj = tf.extractfile(m)
            if fobj is None:
                continue
            data, sr = sf.read(io.BytesIO(fobj.read()), dtype="float32", always_2d=False)
            assert sr == 16000, f"Expected 16000 Hz, got {sr} in {m.name}"
            if data.ndim > 1:
                data = data.mean(axis=1)
            all_samples.append(data)

    if not all_samples:
        raise RuntimeError(f"No audio data extracted for speaker {speaker_id}")

    return np.concatenate(all_samples)


def main() -> None:
    args = parse_args()

    fixtures_other: pathlib.Path = args.fixtures_dir / "other"
    fixtures_other.mkdir(parents=True, exist_ok=True)

    cache_tar: pathlib.Path = args.fixtures_dir / ".mini-librispeech.tar.gz"

    # Download once
    if not cache_tar.exists():
        print(f"Downloading mini-LibriSpeech dev-clean-2 → {cache_tar}")
        download_file(MINI_LIBRI_URL, cache_tar)
    else:
        print(f"Using cached archive: {cache_tar}")

    # Discover available speakers
    print("Scanning archive for speaker IDs…")
    all_speakers = discover_speakers(cache_tar)
    if not all_speakers:
        print("ERROR: No speakers found in archive.", file=sys.stderr)
        sys.exit(1)

    selected = all_speakers[: args.num_speakers]
    print(f"Found {len(all_speakers)} speakers; extracting first {len(selected)}: {selected}")

    # Extract each speaker
    for spk_id in selected:
        out_path = fixtures_other / f"spk_{spk_id}.wav"
        if out_path.exists() and out_path.stat().st_size > 0:
            print(f"  {out_path.name} already exists — skipping")
            continue

        print(f"Extracting speaker {spk_id}…")
        samples = extract_speaker(cache_tar, spk_id)
        sf.write(str(out_path), samples, 16000, subtype="PCM_16")
        duration_s = len(samples) / 16000
        print(f"  Wrote {out_path.name}  ({duration_s:.1f}s, {len(samples):,} samples)")

    print("Done.")


if __name__ == "__main__":
    main()
