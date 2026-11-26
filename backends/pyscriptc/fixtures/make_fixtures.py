"""Generate deterministic 16 kHz mono fixtures for the spike CLI (CPython only).

Usage: python3 backends/pyscriptc/fixtures/make_fixtures.py OUTPUT_DIR
"""

import math
import os
import struct
import sys
import wave

RATE = 16000


def tone_samples(frequency: float, seconds: float) -> list[float]:
    samples: list[float] = []
    count = int(RATE * seconds)
    for i in range(count):
        t = i / RATE
        envelope = 0.5 + 0.5 * math.sin(2.0 * math.pi * 0.7 * t)
        value = 0.45 * math.sin(2.0 * math.pi * frequency * t)
        value += 0.15 * math.sin(2.0 * math.pi * frequency * 2.0 * t) * envelope
        samples.append(value)
    return samples


def write_wav(path: str, samples: list[float]) -> None:
    with wave.open(path, "wb") as handle:
        handle.setnchannels(1)
        handle.setsampwidth(2)
        handle.setframerate(RATE)
        frames = bytearray()
        for sample in samples:
            frames.extend(struct.pack("<h", int(max(-1.0, min(1.0, sample)) * 32767)))
        handle.writeframes(bytes(frames))


def write_f32(path: str, samples: list[float]) -> None:
    with open(path, "wb") as handle:
        for sample in samples:
            handle.write(struct.pack("<f", sample))


def main() -> None:
    if len(sys.argv) != 2:
        print("usage: make_fixtures.py OUTPUT_DIR")
        return
    out_dir = sys.argv[1]
    os.makedirs(out_dir, exist_ok=True)
    enroll = tone_samples(220.0, 3.0)
    test = tone_samples(220.0, 3.0) + tone_samples(700.0, 3.0)
    write_wav(os.path.join(out_dir, "enroll.wav"), enroll)
    write_wav(os.path.join(out_dir, "test.wav"), test)
    write_f32(os.path.join(out_dir, "enroll.f32"), enroll)
    write_f32(os.path.join(out_dir, "test.f32"), test)
    print("wrote enroll.wav test.wav enroll.f32 test.f32 to " + out_dir)


if __name__ == "__main__":
    main()
