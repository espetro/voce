#ifndef VOCE_NATIVE_H
#define VOCE_NATIVE_H

/*
 * Voce native inference shim, C ABI version 1.
 *
 * The engine consumes 16 kHz mono f32 PCM only. Control data is either typed
 * values or UTF-8 JSON byte buffers. This header has no platform includes and
 * no C++ types so it can be declared directly from Rust, C++, TypeScript FFI,
 * Python ctypes/cffi, Go cgo, Zig @cImport, and Odin importc.
 *
 * Ownership and length rules (all functions):
 *   - VoceEngine is opaque. voce_engine_create transfers ownership of the
 *     handle to the caller; voce_engine_destroy consumes it exactly once.
 *     Passing a destroyed or foreign pointer is undefined behaviour.
 *   - The shim never retains caller memory after a call returns. All input
 *     slices (PCM, JSON) are borrowed for the duration of the call only.
 *   - Caller owns every output buffer. The shim writes into it synchronously.
 *     There is no shim allocator API and no buffer returned by pointer.
 *   - Variable-size outputs take `int64_t *in_out_*_len`. On input it is the
 *     caller capacity (elements for f32/struct arrays, bytes for JSON).
 *     On success it is the number of elements written, or bytes written
 *     EXCLUDING the trailing NUL for JSON.
 *   - To query a required size, pass a NULL data pointer and set *len = 0.
 *     The call returns VOCE_OK and writes the required capacity. For JSON
 *     the required capacity includes room for one trailing NUL byte.
 *   - If capacity is too small the shim returns VOCE_ERR_BUFFER_TOO_SMALL,
 *     writes no output data, and updates *len to the required capacity.
 *   - All sample counts and lengths are signed 64-bit; negative values are
 *     invalid. A zero sample count allows a NULL PCM pointer.
 *   - A handle is not thread-safe. Use one handle per thread or serialize
 *     access externally. No callbacks cross this ABI.
 */

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define VOCE_NATIVE_ABI_VERSION 1
#define VOCE_SAMPLE_RATE_HZ 16000
#define VOCE_WINDOW_SAMPLES 48000
#define VOCE_HOP_SAMPLES 24000
#define VOCE_EMBEDDING_DIM 256

#define VOCE_GATE_PASS 1
#define VOCE_GATE_BLOCK 0

typedef enum VoceStatus {
    VOCE_OK = 0,
    VOCE_ERR_NULL_ARGUMENT = -1,
    VOCE_ERR_INVALID_ARGUMENT = -2,
    VOCE_ERR_INVALID_STATE = -3,
    VOCE_ERR_BUFFER_TOO_SMALL = -4,
    VOCE_ERR_NOT_ENROLLED = -5,
    VOCE_ERR_IO = -6,
    VOCE_ERR_UNSUPPORTED = -7
} VoceStatus;

typedef struct VoceEngine VoceEngine;

/* Plain-old-data result; no padding-sensitive booleans or enums.
 * decision is VOCE_GATE_PASS or VOCE_GATE_BLOCK. */
typedef struct VoceWindowResult {
    double runtime_ms;
    float similarity;
    float speech_probability;
    int32_t decision;
} VoceWindowResult;

/* Returns VOCE_NATIVE_ABI_VERSION. Never fails. */
int32_t voce_abi_version(void);

/* Lifecycle. create may return NULL on allocation failure. destroy(NULL) is
 * a no-op. */
VoceEngine *voce_engine_create(void);
void voce_engine_destroy(VoceEngine *engine);

/* Model configuration. models_dir is a NUL-terminated path (UTF-8 or native
 * encoding). The mock backend accepts any non-empty path and does not read
 * files. A real backend loads its VAD and embedding models here. */
int32_t voce_engine_load_models(VoceEngine *engine, const char *models_dir);

/* Gate threshold in [0, 1]. Similarity >= threshold produces VOCE_GATE_PASS. */
int32_t voce_engine_set_threshold(VoceEngine *engine, float threshold);

/* Enrollment: reset drops any previous profile and buffered audio. enroll_pcm
 * may be called any number of times before finish_enrollment. */
int32_t voce_engine_reset_enrollment(VoceEngine *engine);
int32_t voce_engine_enroll_pcm(VoceEngine *engine, const float *pcm,
                               int64_t n_samples);

/* Produces the profile embedding. out_embedding may be NULL for a size query.
 * Requires at least one non-empty enrollment chunk. */
int32_t voce_engine_finish_enrollment(VoceEngine *engine,
                                      float *out_embedding,
                                      int64_t *in_out_embedding_len);

/* Reports the fixed embedding dimension (VOCE_EMBEDDING_DIM). */
int32_t voce_engine_embedding_dim(VoceEngine *engine, int64_t *out_dim);

/* Scores one window (VOCE_WINDOW_SAMPLES samples expected) against the
 * enrolled profile. Non-speech windows return similarity 0 and PASS. */
int32_t voce_engine_score_window(VoceEngine *engine, const float *pcm,
                                 int64_t n_samples,
                                 VoceWindowResult *out_result);

/* Batch scoring: complete VOCE_WINDOW_SAMPLES windows at VOCE_HOP_SAMPLES
 * spacing. out_results may be NULL for a size query. out_windows_total may be
 * NULL; otherwise it receives the number of complete windows regardless of
 * the caller capacity. */
int32_t voce_engine_process_pcm(VoceEngine *engine, const float *pcm,
                                int64_t n_samples,
                                VoceWindowResult *out_results,
                                int64_t *in_out_results_len,
                                int64_t *out_windows_total);

/* Profile state as JSON bytes. On success in_out_json_capacity_bytes is bytes
 * written excluding NUL; the buffer is NUL-terminated within capacity.
 * deserialize takes an explicit byte length; embedded NULs are rejected. A
 * zero json_len_bytes means the string is NUL-terminated. */
int32_t voce_engine_serialize_profile_json(VoceEngine *engine,
                                           char *out_json,
                                           int64_t *in_out_json_capacity_bytes);
int32_t voce_engine_deserialize_profile_json(VoceEngine *engine,
                                             const char *json,
                                             int64_t json_len_bytes);

#ifdef __cplusplus
}
#endif

#endif /* VOCE_NATIVE_H */
