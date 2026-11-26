"""Voce pyscriptc backend spike: typed orchestration over a local shim contract.

Audio contract: 16 kHz mono f32, 48000-sample (3 s) windows, 16000-sample hop.
Runtime inference stays behind NativeAdapter; make_adapter() is the only swap
point between the deterministic mock and a future voce_native FFI module.
"""

import json
import math
import os
import struct
import sys
import time

import voce_native

SAMPLE_RATE: int = 16000
WINDOW_SAMPLES: int = 48000
HOP_SAMPLES: int = 16000
EMBEDDING_DIM: int = 16
DECIMATION: int = 4
MAX_ENROLL_WINDOWS: int = 3
DEFAULT_THRESHOLD: float = 0.75
DEFAULT_VOTE_WINDOW: int = 3
TWO_PI: float = 6.283185307179586


class Config:
    def __init__(
        self,
        input_path: str,
        enroll_path: str,
        threshold: float,
        vote_window: int,
        repeat: int,
        error: str,
    ) -> None:
        self.input_path: str = input_path
        self.enroll_path: str = enroll_path
        self.threshold: float = threshold
        self.vote_window: int = vote_window
        self.repeat: int = repeat
        self.error: str = error


class Profile:
    def __init__(
        self, embedding: list[float], windows: int, threshold: float, vote_window: int
    ) -> None:
        self.embedding: list[float] = embedding
        self.windows: int = windows
        self.threshold: float = threshold
        self.vote_window: int = vote_window

    def to_json(self) -> str:
        return (
            '{"version":1,"sample_rate":'
            + str(SAMPLE_RATE)
            + ',"embedding_dim":'
            + str(len(self.embedding))
            + ',"embedding":'
            + json.dumps(self.embedding)
            + ',"enrollment_windows":'
            + str(self.windows)
            + ',"threshold":'
            + str(self.threshold)
            + ',"vote_window":'
            + str(self.vote_window)
            + "}"
        )


class RunResult:
    def __init__(
        self, windows_json: str, profile_json: str, decisions: str, total_ms: float
    ) -> None:
        self.windows_json: str = windows_json
        self.profile_json: str = profile_json
        self.decisions: str = decisions
        self.total_ms: float = total_ms


class MockNativeAdapter:
    """Local mirror of the intended voce_native adapter contract.

    embed_window() returns a unit-length embedding for one 3 s window.
    similarity() compares a window embedding to the enrolled profile.
    A real adapter must expose exactly these two methods; make_adapter() is
    the only orchestration site that names the implementation.
    """
    """Deterministic mock: normalised band magnitudes, cosine-style dot product.

    The same routine runs for enrollment and scoring, so a window is compared
    against the enrolled profile rather than against a per-window hash.
    """

    def __init__(self, dim: int) -> None:
        self.dim: int = dim

    def embed_window(self, window: list[float]) -> list[float]:
        feats: list[float] = []
        k: int = 0
        while k < self.dim:
            freq: float = 100.0 + 110.0 * float(k)
            re: float = 0.0
            im: float = 0.0
            i: int = 0
            while i < len(window):
                sample: float = window[i]
                angle: float = TWO_PI * freq * (float(i) / float(SAMPLE_RATE))
                re += sample * math.cos(angle)
                im += sample * math.sin(angle)
                i += DECIMATION
            feats.append(math.sqrt(re * re + im * im))
            k += 1
        norm: float = 0.0
        for value in feats:
            norm += value * value
        norm = math.sqrt(norm)
        result: list[float] = []
        if norm > 1e-9:
            for value in feats:
                result.append(value / norm)
        else:
            i = 0
            while i < self.dim:
                result.append(0.0)
                i += 1
        return result

    def similarity(self, profile: list[float], window: list[float]) -> float:
        dot: float = 0.0
        i: int = 0
        while i < len(profile) and i < len(window):
            dot += profile[i] * window[i]
            i += 1
        return dot


def make_adapter() -> MockNativeAdapter:
    """Single swap point for the real voce_native FFI adapter."""
    return MockNativeAdapter(EMBEDDING_DIM)


def bool_text(value: bool) -> str:
    if value:
        return "true"
    return "false"


def find_from(text: str, needle: str, start: int) -> int:
    i: int = start
    while i + len(needle) <= len(text):
        if text[i : i + len(needle)] == needle:
            return i
        i += 1
    return -1


def number_field(text: str, key: str) -> float:
    marker: str = '"' + key + '":'
    start: int = find_from(text, marker, 0)
    end: int = find_from(text, ",", start)
    if end < 0:
        end = find_from(text, "}", start)
    return float(text[start + len(marker) : end])


def parse_profile(text: str) -> Profile:
    start: int = find_from(text, '"embedding":[', 0)
    begin: int = find_from(text, "[", start)
    end: int = find_from(text, "]", begin)
    vec: list[float] = json.loads(text[begin : end + 1])
    return Profile(
        vec,
        int(number_field(text, "enrollment_windows")),
        number_field(text, "threshold"),
        int(number_field(text, "vote_window")),
    )


def byte_list(data: bytes) -> list[int]:
    raw: list[int] = []
    for value in data:
        raw.append(value)
    return raw


def little_int(raw: list[int], offset: int, width: int) -> int:
    result: int = 0
    i: int = 0
    while i < width:
        result += raw[offset + i] * (256 ** i)
        i += 1
    return result


def f32_from_bits(bits: int) -> float:
    packed: bytes = struct.pack("<I", bits)
    return struct.unpack("<f", packed)[0]


def read_f32(raw: list[int]) -> list[float]:
    pcm: list[float] = []
    count: int = len(raw) // 4
    i: int = 0
    while i < count:
        pcm.append(f32_from_bits(little_int(raw, i * 4, 4)))
        i += 1
    return pcm


def is_chunk(raw: list[int], pos: int, name: str) -> bool:
    i: int = 0
    while i < len(name):
        if raw[pos + i] != ord(name[i]):
            return False
        i += 1
    return True


def read_wav(raw: list[int]) -> list[float]:
    pos: int = 12
    rate: int = 0
    channels: int = 0
    bits: int = 0
    fmt: int = 0
    start: int = 0
    size: int = 0
    while pos + 8 <= len(raw):
        chunk_size: int = little_int(raw, pos + 4, 4)
        body: int = pos + 8
        if is_chunk(raw, pos, "fmt "):
            fmt = little_int(raw, body, 2)
            channels = little_int(raw, body + 2, 2)
            rate = little_int(raw, body + 4, 4)
            bits = little_int(raw, body + 14, 2)
        elif is_chunk(raw, pos, "data"):
            start = body
            size = chunk_size
            break
        pos = body + chunk_size + (chunk_size % 2)
    if rate != SAMPLE_RATE or channels != 1:
        return []
    pcm: list[float] = []
    i: int = 0
    if bits == 16 and fmt == 1:
        while i < size // 2:
            value: int = little_int(raw, start + i * 2, 2)
            if value >= 32768:
                value -= 65536
            pcm.append(float(value) / 32768.0)
            i += 1
    elif bits == 32 and fmt == 3:
        while i < size // 4:
            pcm.append(f32_from_bits(little_int(raw, start + i * 4, 4)))
            i += 1
    return pcm


def read_pcm(path: str) -> list[float]:
    with open(path, "rb") as handle:
        data: bytes = handle.read()
    raw: list[int] = byte_list(data)
    if (
        len(raw) >= 12
        and is_chunk(raw, 0, "RIFF")
        and is_chunk(raw, 8, "WAVE")
    ):
        return read_wav(raw)
    return read_f32(raw)


def make_windows(pcm: list[float]) -> list[list[float]]:
    windows: list[list[float]] = []
    if len(pcm) < WINDOW_SAMPLES:
        return windows
    start: int = 0
    while start + WINDOW_SAMPLES <= len(pcm):
        windows.append(pcm[start : start + WINDOW_SAMPLES])
        start += HOP_SAMPLES
    return windows


def enroll(
    adapter: MockNativeAdapter, windows: list[list[float]], threshold: float, vote_window: int
) -> Profile:
    total: list[float] = []
    k: int = 0
    while k < EMBEDDING_DIM:
        total.append(0.0)
        k += 1
    count: int = 0
    for window in windows:
        if count >= MAX_ENROLL_WINDOWS:
            break
        embedding: list[float] = adapter.embed_window(window)
        i: int = 0
        while i < EMBEDDING_DIM:
            total[i] += embedding[i]
            i += 1
        count += 1
    norm: float = 0.0
    for value in total:
        norm += value * value
    norm = math.sqrt(norm)
    profile_vec: list[float] = []
    if norm > 1e-9:
        for value in total:
            profile_vec.append(value / norm)
    else:
        k = 0
        while k < EMBEDDING_DIM:
            profile_vec.append(0.0)
            k += 1
    return Profile(profile_vec, count, threshold, vote_window)


def window_json(
    index: int, similarity: float, passed: bool, gated: bool, runtime_ms: float
) -> str:
    return (
        '{"index":'
        + str(index)
        + ',"similarity":'
        + str(similarity)
        + ',"pass":'
        + bool_text(passed)
        + ',"gated":'
        + bool_text(gated)
        + ',"runtime_ms":'
        + str(runtime_ms)
        + "}"
    )


def decision_digest(decisions: str) -> str:
    digest: int = 2166136261
    i: int = 0
    while i < len(decisions):
        digest = (digest * 16777619) % 4294967296
        digest = digest ^ ord(decisions[i])
        i += 1
    return str(digest)


def run_once(
    adapter: MockNativeAdapter,
    windows: list[list[float]],
    profile: Profile,
    threshold: float,
    vote_window: int,
) -> RunResult:
    items: list[str] = []
    decisions: str = ""
    recent: list[bool] = []
    total_ms: float = 0.0
    index: int = 0
    for window in windows:
        started: float = time.perf_counter()
        embedding: list[float] = adapter.embed_window(window)
        similarity: float = adapter.similarity(profile.embedding, embedding)
        elapsed_ms: float = (time.perf_counter() - started) * 1000.0
        passed: bool = similarity >= threshold
        decisions += "1" if passed else "0"
        recent.append(passed)
        if len(recent) > vote_window:
            recent = recent[len(recent) - vote_window :]
        votes: int = 0
        vote_index: int = 0
        while vote_index < len(recent):
            if recent[vote_index]:
                votes += 1
            vote_index += 1
        gated: bool = votes * 2 >= vote_window
        items.append(window_json(index, similarity, passed, gated, elapsed_ms))
        total_ms += elapsed_ms
        index += 1
    joined: str = ""
    for item in items:
        if joined != "":
            joined += ","
        joined += item
    return RunResult(joined, profile.to_json(), decisions, total_ms)


def parse_args(argv: list[str]) -> Config:
    input_path: str = ""
    enroll_path: str = ""
    threshold: float = DEFAULT_THRESHOLD
    vote_window: int = DEFAULT_VOTE_WINDOW
    repeat: int = 2
    error: str = ""
    i: int = 1
    while i < len(argv):
        arg: str = argv[i]
        if arg == "--help":
            error = "usage: voce_spike --input FILE [--enroll FILE] [--threshold F] [--vote-window N] [--repeat N]"
            return Config("", "", threshold, vote_window, repeat, error)
        if i + 1 >= len(argv):
            error = "missing value for " + arg
            break
        value: str = argv[i + 1]
        if arg == "--input":
            input_path = value
        elif arg == "--enroll":
            enroll_path = value
        elif arg == "--threshold":
            threshold = float(value)
        elif arg == "--vote-window":
            vote_window = int(value)
        elif arg == "--repeat":
            repeat = int(value)
        else:
            error = "unknown flag " + arg
            break
        i += 2
    if input_path == "" and error == "":
        error = "--input is required"
    return Config(input_path, enroll_path, threshold, vote_window, repeat, error)


def main() -> None:
    config: Config = parse_args(sys.argv)
    if config.error != "":
        print(config.error)
        return
    pcm: list[float] = read_pcm(config.input_path)
    if len(pcm) < WINDOW_SAMPLES:
        print("input must contain at least 48000 samples of 16 kHz mono f32 audio")
        return
    windows: list[list[float]] = make_windows(pcm)
    enroll_pcm: list[float] = pcm
    if config.enroll_path != "":
        enroll_pcm = read_pcm(config.enroll_path)
    enroll_windows: list[list[float]] = make_windows(enroll_pcm)
    if len(enroll_windows) == 0:
        print("enrollment input must contain at least 48000 samples")
        return
    adapter: MockNativeAdapter = make_adapter()
    profile: Profile = enroll(adapter, enroll_windows, config.threshold, config.vote_window)
    first: RunResult = run_once(
        adapter, windows, profile, config.threshold, config.vote_window
    )
    parsed: Profile = parse_profile(first.profile_json)
    profile_round_trip: bool = (
        len(parsed.embedding) == EMBEDDING_DIM
        and parsed.windows == profile.windows
        and parsed.vote_window == profile.vote_window
    )
    repeat: int = config.repeat
    if repeat < 1:
        repeat = 1
    deterministic: bool = True
    run: int = 1
    while run < repeat:
        again: RunResult = run_once(
            adapter, windows, profile, config.threshold, config.vote_window
        )
        if again.decisions != first.decisions or again.profile_json != first.profile_json:
            deterministic = False
        run += 1
    artifact_bytes: int = 0
    try:
        artifact_bytes = os.stat(sys.argv[0]).st_size
    except OSError:
        artifact_bytes = 0
    print(
        '{"schema":"voce-pyscriptc-spike/1","input":'
        + json.dumps(config.input_path)
        + ',"sample_rate":'
        + str(SAMPLE_RATE)
        + ',"window_samples":'
        + str(WINDOW_SAMPLES)
        + ',"hop_samples":'
        + str(HOP_SAMPLES)
        + ',"embedding_dim":'
        + str(EMBEDDING_DIM)
        + ',"threshold":'
        + str(config.threshold)
        + ',"vote_window":'
        + str(config.vote_window)
        + ',"enrollment_windows":'
        + str(profile.windows)
        + ',"profile_round_trip":'
        + bool_text(profile_round_trip)
        + ',"profile":'
        + first.profile_json
        + ',"windows":['
        + first.windows_json
        + '],"runtime_ms_total":'
        + str(first.total_ms)
        + ',"rss_kb":'
        + str(voce_native.rss_kb())
        + ',"artifact_size_bytes":'
        + str(artifact_bytes)
        + ',"repeat":'
        + str(repeat)
        + ',"decision_digest":'
        + json.dumps(decision_digest(first.decisions))
        + ',"deterministic":'
        + bool_text(deterministic)
        + "}"
    )


if __name__ == "__main__":
    main()
