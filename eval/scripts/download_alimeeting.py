#!/usr/bin/env python3
"""Copies AliMeeting evaluation dataset from local download to eval fixtures."""

import os
import shutil
import json
import soundfile as sf
from pathlib import Path
from typing import Dict, List, Any, Tuple

SOURCE_DIR = Path("/Users/joaquin.terrasamoya/Downloads/Eval_Ali/Eval_Ali_far/")
TARGET_DIR = Path("eval/fixtures/alimeeting/")
AUDIO_SUBDIR = "audio_dir"
TEXTGRID_SUBDIR = "textgrid_dir"
TARGET_AUDIO = "audio"
TARGET_TEXTGRID = "textgrid"

TARGET_AUDIO_DIR = TARGET_DIR / TARGET_AUDIO
TARGET_TEXTGRID_DIR = TARGET_DIR / TARGET_TEXTGRID


def setup_directories():
    TARGET_AUDIO_DIR.mkdir(parents=True, exist_ok=True)
    TARGET_TEXTGRID_DIR.mkdir(parents=True, exist_ok=True)


def copy_file_with_check(src: Path, dst: Path) -> bool:
    if dst.exists():
        return False
    shutil.copy2(src, dst)
    return True


def extract_base_name(audio_filename: str) -> str:
    name = audio_filename[:-4]
    parts = name.rsplit("_", 1)
    if len(parts) == 2 and parts[1].startswith("MS"):
        return parts[0]
    return name


def verify_wav(file_path: Path) -> Dict[str, Any]:
    info = sf.info(str(file_path))
    return {
        "sample_rate": info.samplerate,
        "channels": info.channels,
        "duration_s": info.duration,
        "format": info.format,
        "subtype": info.subtype,
    }


def verify_textgrid(file_path: Path) -> Dict[str, Any]:
    try:
        with open(file_path, "r", encoding="utf-8") as f:
            content = f.read()

        speaker_lines = [
            line
            for line in content.split("\n")
            if "name = " in line.lower() or "xmin" in line
        ]
        speakers = []
        for line in content.split("\n"):
            if 'name = "' in line:
                try:
                    name = line.split('"')[1]
                    if name.startswith("N_SPK") or name.startswith("SPK"):
                        speakers.append(name)
                except (IndexError, AttributeError):
                    continue

        return {
            "parseable": True,
            "speakers": list(set(speakers)),
            "speaker_count": len(set(speakers)),
        }
    except Exception as e:
        return {"parseable": False, "error": str(e), "speakers": [], "speaker_count": 0}


def get_audio_files(source_dir: Path) -> List[Path]:
    audio_dir = source_dir / AUDIO_SUBDIR
    if not audio_dir.exists():
        raise FileNotFoundError(f"Audio directory not found: {audio_dir}")
    return sorted(list(audio_dir.glob("*.wav")))


def get_textgrid_files(source_dir: Path) -> List[Path]:
    textgrid_dir = source_dir / TEXTGRID_SUBDIR
    if not textgrid_dir.exists():
        raise FileNotFoundError(f"TextGrid directory not found: {textgrid_dir}")
    return sorted(list(textgrid_dir.glob("*.TextGrid")))


def copy_and_verify_audio(
    audio_files: List[Path],
) -> Tuple[List[Dict[str, Any]], float]:
    audio_metadata = []
    total_duration = 0.0

    for audio_file in audio_files:
        target_file = TARGET_AUDIO_DIR / audio_file.name

        copied = copy_file_with_check(audio_file, target_file)

        try:
            audio_info = verify_wav(target_file)

            if audio_info["sample_rate"] != 16000:
                print(
                    f"  WARNING: {audio_file.name} has sample rate {audio_info['sample_rate']} (expected 16000)"
                )
            if audio_info["channels"] != 8:
                print(
                    f"  WARNING: {audio_file.name} has {audio_info['channels']} channels (expected 8)"
                )

            base_name = extract_base_name(audio_file.name)
            metadata = {
                "filename": audio_file.name,
                "duration_s": round(audio_info["duration_s"], 2),
                "sample_rate": audio_info["sample_rate"],
                "channels": audio_info["channels"],
                "copied": copied,
            }

            audio_metadata.append(metadata)
            total_duration += audio_info["duration_s"]

            if copied:
                print(
                    f"  Copied: {audio_file.name} ({round(audio_info['duration_s'], 2)}s)"
                )
            else:
                print(
                    f"  Exists: {audio_file.name} ({round(audio_info['duration_s'], 2)}s)"
                )

        except Exception as e:
            print(f"  ERROR: Failed to verify {audio_file.name}: {e}")
            raise

    return audio_metadata, total_duration


def copy_and_verify_textgrid(textgrid_files: List[Path]) -> List[Dict[str, Any]]:
    textgrid_metadata = []

    for textgrid_file in textgrid_files:
        target_file = TARGET_TEXTGRID_DIR / textgrid_file.name

        copied = copy_file_with_check(textgrid_file, target_file)

        try:
            tg_info = verify_textgrid(target_file)

            if not tg_info["parseable"]:
                print(
                    f"  WARNING: {textgrid_file.name} could not be parsed: {tg_info.get('error', 'Unknown error')}"
                )

            metadata = {
                "filename": textgrid_file.name,
                "speakers": tg_info["speakers"],
                "copied": copied,
            }

            textgrid_metadata.append(metadata)

            if copied:
                print(
                    f"  Copied: {textgrid_file.name} ({tg_info['speaker_count']} speakers)"
                )
            else:
                print(
                    f"  Exists: {textgrid_file.name} ({tg_info['speaker_count']} speakers)"
                )

        except Exception as e:
            print(f"  ERROR: Failed to verify {textgrid_file.name}: {e}")
            raise

    return textgrid_metadata


def match_files(
    audio_metadata: List[Dict[str, Any]], textgrid_metadata: List[Dict[str, Any]]
) -> List[Dict[str, Any]]:
    matches = []

    for audio_meta in audio_metadata:
        audio_filename = audio_meta["filename"]
        base_name = extract_base_name(audio_filename)

        matching_tg = None
        for tg_meta in textgrid_metadata:
            tg_base_name = tg_meta["filename"][:-9]
            if tg_base_name == base_name:
                matching_tg = tg_meta
                break

        if matching_tg:
            matches.append(
                {
                    "filename": audio_filename,
                    "duration_s": audio_meta["duration_s"],
                    "sample_rate": audio_meta["sample_rate"],
                    "channels": audio_meta["channels"],
                    "speakers": len(matching_tg["speakers"]),
                }
            )

    return matches


def generate_manifest(
    source_path: str,
    audio_metadata: List[Dict[str, Any]],
    textgrid_metadata: List[Dict[str, Any]],
    matched_files: List[Dict[str, Any]],
) -> Dict[str, Any]:
    return {
        "source_path": source_path,
        "audio_files": matched_files,
        "textgrid_files": [
            {
                "filename": tg["filename"],
                "audio_match": extract_base_name(tg["filename"]) + "_MSxxx.wav",
                "speakers": tg["speakers"],
            }
            for tg in textgrid_metadata
        ],
    }


def print_summary(audio_count: int, textgrid_count: int, total_duration: float):
    print("\n" + "=" * 60)
    print("ALIMEETING DATASET SETUP COMPLETE")
    print("=" * 60)
    print(f"Audio files:      {audio_count}")
    print(f"TextGrid files:   {textgrid_count}")
    print(
        f"Total duration:   {round(total_duration / 60, 2)} minutes ({round(total_duration, 2)} seconds)"
    )
    print(f"\nTarget directory: {TARGET_DIR.resolve()}")
    print("=" * 60)


def main():
    if not SOURCE_DIR.exists():
        print(f"ERROR: Source directory not found: {SOURCE_DIR}")
        print(
            "Please ensure the AliMeeting dataset is downloaded to the expected location."
        )
        return 1

    print(f"Source directory: {SOURCE_DIR}")
    print(f"Target directory: {TARGET_DIR.resolve()}")

    setup_directories()
    print("\nDirectories created/verified.")

    print("\nScanning source files...")
    audio_files = get_audio_files(SOURCE_DIR)
    textgrid_files = get_textgrid_files(SOURCE_DIR)

    print(f"Found {len(audio_files)} audio files")
    print(f"Found {len(textgrid_files)} TextGrid files")

    print("\nProcessing audio files...")
    audio_metadata, total_duration = copy_and_verify_audio(audio_files)

    print("\nProcessing TextGrid files...")
    textgrid_metadata = copy_and_verify_textgrid(textgrid_files)

    matched_files = match_files(audio_metadata, textgrid_metadata)

    print("\nGenerating manifest.json...")
    manifest = generate_manifest(
        source_path=str(SOURCE_DIR.resolve()),
        audio_metadata=audio_metadata,
        textgrid_metadata=textgrid_metadata,
        matched_files=matched_files,
    )

    manifest_file = TARGET_DIR / "manifest.json"
    with open(manifest_file, "w") as f:
        json.dump(manifest, f, indent=2)

    print(f"  Manifest written to: {manifest_file}")

    print_summary(len(audio_metadata), len(textgrid_metadata), total_duration)

    return 0


if __name__ == "__main__":
    exit(main())
