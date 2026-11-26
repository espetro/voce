"""Minimal pyscriptc consumer of the reviewed native adapter.

Only byte-frame construction and f64-to-f32 conversion remain in this lane;
capacity queries, element counts and native output buffers are the adapter's
job. The entry module stays self-contained because pyscriptc's closed import
table rejects local modules.
"""

import voce_adapter

WINDOW_SAMPLES = 48000
HOP_SAMPLES = 24000
RESULT_BYTES = 24


def push_i64(frame: list[int], value: int) -> None:
    i = 0
    while i < 8:
        frame.append((value >> (i * 8)) & 255)
        i += 1


def put_i64(frame: list[int], offset: int, value: int) -> None:
    i = 0
    while i < 8:
        frame[offset + i] = (value >> (i * 8)) & 255
        i += 1


def read_i64(frame: list[int]) -> int:
    value = 0
    i = 0
    while i < 8:
        value = value | (frame[i] << (i * 8))
        i += 1
    return value


def read_u32(frame: list[int], offset: int) -> int:
    value = 0
    i = 0
    while i < 4:
        value = value | (frame[offset + i] << (i * 8))
        i += 1
    return value


def push_f32(frame: list[int], bits: int) -> None:
    i = 0
    while i < 4:
        frame.append((bits >> (i * 8)) & 255)
        i += 1


def push_pcm(frame: list[int], samples: int) -> None:
    i = 0
    while i < samples:
        push_f32(frame, 1048576000)
        i += 1


def check(code: int, label: int) -> None:
    if code != 0:
        print(label)
        print(code)


def run() -> None:
    session = voce_adapter.create()
    path: list[int] = [109, 111, 99, 107, 58, 47, 47, 98, 117, 105, 108, 116, 105, 110, 0]
    check(voce_adapter.call(session, 1, path), 1)
    threshold: list[int] = []
    push_f32(threshold, 1061158912)
    check(voce_adapter.call(session, 2, threshold), 2)
    check(voce_adapter.call(session, 3, []), 3)

    enroll: list[int] = []
    push_pcm(enroll, WINDOW_SAMPLES)
    check(voce_adapter.call(session, 4, enroll), 4)

    query: list[int] = []
    push_i64(query, 0)
    check(voce_adapter.call(session, 5, query), 5)
    dim = read_i64(query)
    embedding: list[int] = []
    push_i64(embedding, dim)
    i = 0
    while i < dim * 4:
        embedding.append(0)
        i += 1
    check(voce_adapter.call(session, 6, embedding), 6)

    score_frame: list[int] = []
    push_pcm(score_frame, WINDOW_SAMPLES)
    i = 0
    while i < RESULT_BYTES:
        score_frame.append(0)
        i += 1
    check(voce_adapter.call(session, 8, score_frame), 8)
    similarity_bits = read_u32(score_frame, WINDOW_SAMPLES * 4 + 8)

    batch: list[int] = []
    push_i64(batch, 0)
    push_pcm(batch, WINDOW_SAMPLES + HOP_SAMPLES)
    check(voce_adapter.call(session, 9, batch), 9)
    windows = read_i64(batch)

    results: list[int] = []
    push_i64(results, windows)
    push_pcm(results, WINDOW_SAMPLES + HOP_SAMPLES)
    i = 0
    while i < windows * RESULT_BYTES:
        results.append(0)
        i += 1
    check(voce_adapter.call(session, 10, results), 10)

    json_query: list[int] = []
    push_i64(json_query, 0)
    check(voce_adapter.call(session, 11, json_query), 11)
    capacity = read_i64(json_query)
    json_frame: list[int] = []
    push_i64(json_frame, capacity)
    i = 0
    while i < capacity:
        json_frame.append(0)
        i += 1
    check(voce_adapter.call(session, 12, json_frame), 12)
    json_bytes = read_i64(json_frame)

    restored = voce_adapter.create()
    check(voce_adapter.call(restored, 1, path), 13)
    json: list[int] = []
    i = 0
    while i < json_bytes:
        json.append(json_frame[8 + i])
        i += 1
    check(voce_adapter.call(restored, 13, json), 13)
    restored_score: list[int] = []
    push_pcm(restored_score, WINDOW_SAMPLES)
    i = 0
    while i < RESULT_BYTES:
        restored_score.append(0)
        i += 1
    check(voce_adapter.call(restored, 8, restored_score), 8)
    restored_bits = read_u32(restored_score, WINDOW_SAMPLES * 4 + 8)

    voce_adapter.destroy(restored)
    voce_adapter.destroy(session)
    print(dim)
    print(windows)
    print(json_bytes)
    print(similarity_bits)
    print(restored_bits)


run()
