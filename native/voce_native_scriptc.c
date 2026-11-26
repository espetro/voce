/*
 * Thin adaptation layer for scriptc FFI (C11).
 *
 * The Rust cdylib loads and links directly, but scriptc cannot express
 * `int64_t *in_out_*_len` output parameters: `mutable-bytes` lowers to
 * `(ptr, byte_length)`, so a native length pointer receives the byte length
 * as its address and the process segfaults (see
 * docs/research/scriptc-native-integration.md, experiment 3). Byte-span
 * lengths are in bytes, while the ABI's PCM counts are in f32 elements.
 *
 * This wrapper exposes exactly two bindings to TypeScript:
 *
 *   int32_t  vws_create(void);            // session id >= 1, or < 0 = status
 *   void     vws_destroy(int32_t session);
 *   int32_t  vws_call(int32_t session, uint32_t op, uint8_t *frame,
 *                     int64_t frame_len); // 'mutable-bytes' in the manifest
 *
 * Every `vws_call` frame is little-endian and caller-owned. One byte span
 * per call keeps scriptc's FFI lowering simple. Status codes are the
 * VoceStatus values from voce_native.h, plus -3 for an invalid session.
 *
 * Frame layouts (all i64 prefix fields are 8-byte little-endian):
 *   1 LOAD_MODELS      path bytes, including trailing NUL
 *   2 SET_THRESHOLD    4 bytes = f32 threshold in [0, 1]
 *   3 RESET            empty
 *   4 ENROLL_PCM       f32 samples; frame_len must be a multiple of 4
 *   5 FINISH_QUERY     8-byte zero prefix -> required f32 element count
 *   6 FINISH           [capacity i64][embedding f32 * capacity]; prefix
 *                      receives the number of f32 values written
 *   7 EMBEDDING_DIM    8 bytes -> i64 dimension
 *   8 SCORE_WINDOW     [f32 pcm samples][24-byte result]; result is written
 *                      into the trailing 24 bytes (double, f32, f32, i32)
 *   9 PROCESS_QUERY    [8-byte i64 prefix][f32 pcm samples]; prefix receives
 *                      the complete window count; out_results stays NULL
 *  10 PROCESS_PCM      [capacity i64][f32 pcm][24-byte result * capacity];
 *                      prefix receives the number of results written
 * 11 SERIALIZE_QUERY  8-byte zero prefix -> required JSON bytes incl. NUL
 * 12 SERIALIZE_JSON   [capacity i64][json bytes]; prefix receives bytes
 *                      written, excluding the trailing NUL
 * 13 DESERIALIZE_JSON JSON bytes, no trailing NUL required
 *
 * Queries call the real ABI with a NULL data pointer and *len = 0, exactly
 * as documented in voce_native.h. Writes call the real ABI with a
 * caller-sized buffer; a too-small capacity returns -4 without writing.
 */

#include "voce_native.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

enum {
    VWS_LOAD_MODELS = 1,
    VWS_SET_THRESHOLD = 2,
    VWS_RESET = 3,
    VWS_ENROLL_PCM = 4,
    VWS_FINISH_QUERY = 5,
    VWS_FINISH = 6,
    VWS_EMBEDDING_DIM = 7,
    VWS_SCORE_WINDOW = 8,
    VWS_PROCESS_QUERY = 9,
    VWS_PROCESS_PCM = 10,
    VWS_SERIALIZE_QUERY = 11,
    VWS_SERIALIZE_JSON = 12,
    VWS_DESERIALIZE_JSON = 13,
};

#define VWS_MAX_SESSIONS 16
#define VWS_RESULT_BYTES 24

static VoceEngine *sessions[VWS_MAX_SESSIONS];

int32_t vws_create(void) {
    for (int i = 0; i < VWS_MAX_SESSIONS; i++) {
        if (sessions[i] == NULL) {
            sessions[i] = voce_engine_create();
            if (sessions[i] == NULL) return VOCE_ERR_IO;
            return (int32_t)(i + 1);
        }
    }
    return VOCE_ERR_UNSUPPORTED;
}

void vws_destroy(int32_t session) {
    if (session < 1 || session > VWS_MAX_SESSIONS) return;
    VoceEngine *engine = sessions[session - 1];
    if (engine != NULL) {
        voce_engine_destroy(engine);
        sessions[session - 1] = NULL;
    }
}

static VoceEngine *vws_engine(int32_t session) {
    if (session < 1 || session > VWS_MAX_SESSIONS) return NULL;
    return sessions[session - 1];
}

static int64_t vws_load_i64(const uint8_t *p) {
    int64_t value;
    memcpy(&value, p, sizeof(value));
    return value;
}

static void vws_store_i64(uint8_t *p, int64_t value) {
    memcpy(p, &value, sizeof(value));
}

int32_t vws_call(int32_t session, uint32_t op, uint8_t *frame, int64_t frame_len) {
    VoceEngine *engine = vws_engine(session);
    if (engine == NULL) return VOCE_ERR_INVALID_STATE;
    if (frame_len < 0) return VOCE_ERR_INVALID_ARGUMENT;

    switch (op) {
        case VWS_LOAD_MODELS:
            if (frame == NULL || frame_len == 0 || frame[frame_len - 1] != 0) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            return voce_engine_load_models(engine, (const char *)frame);

        case VWS_SET_THRESHOLD: {
            if (frame == NULL || frame_len < 4) return VOCE_ERR_INVALID_ARGUMENT;
            float threshold;
            memcpy(&threshold, frame, sizeof(threshold));
            return voce_engine_set_threshold(engine, threshold);
        }

        case VWS_RESET:
            return voce_engine_reset_enrollment(engine);

        case VWS_ENROLL_PCM:
            if (frame_len % 4 != 0) return VOCE_ERR_INVALID_ARGUMENT;
            if (frame_len == 0) return voce_engine_enroll_pcm(engine, NULL, 0);
            return voce_engine_enroll_pcm(engine, (const float *)frame, frame_len / 4);

        case VWS_FINISH_QUERY: {
            if (frame == NULL || frame_len < 8) return VOCE_ERR_INVALID_ARGUMENT;
            vws_store_i64(frame, 0);
            int64_t capacity = 0;
            int32_t status = voce_engine_finish_enrollment(engine, NULL, &capacity);
            vws_store_i64(frame, capacity);
            return status;
        }

        case VWS_FINISH: {
            if (frame == NULL || frame_len < 8) return VOCE_ERR_INVALID_ARGUMENT;
            int64_t capacity = vws_load_i64(frame);
            if (capacity < 0 || frame_len - 8 < capacity * 4) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            int64_t written = capacity;
            int32_t status = voce_engine_finish_enrollment(
                engine, (float *)(frame + 8), &written);
            vws_store_i64(frame, written);
            return status;
        }

        case VWS_EMBEDDING_DIM: {
            if (frame == NULL || frame_len < 8) return VOCE_ERR_INVALID_ARGUMENT;
            int64_t dim = 0;
            int32_t status = voce_engine_embedding_dim(engine, &dim);
            vws_store_i64(frame, dim);
            return status;
        }

        case VWS_SCORE_WINDOW: {
            if (frame == NULL || frame_len < VWS_RESULT_BYTES ||
                (frame_len - VWS_RESULT_BYTES) % 4 != 0) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            int64_t samples = (frame_len - VWS_RESULT_BYTES) / 4;
            VoceWindowResult result;
            memset(&result, 0, sizeof(result));
            int32_t status = voce_engine_score_window(
                engine, (const float *)frame, samples, &result);
            if (status == VOCE_OK) {
                memcpy(frame + frame_len - VWS_RESULT_BYTES, &result, sizeof(result));
            }
            return status;
        }

        case VWS_PROCESS_QUERY: {
            if (frame == NULL || frame_len < 8 ||
                (frame_len - 8) % 4 != 0) return VOCE_ERR_INVALID_ARGUMENT;
            int64_t samples = (frame_len - 8) / 4;
            int64_t capacity = 0;
            int64_t total = 0;
            int32_t status = voce_engine_process_pcm(
                engine, (const float *)(frame + 8), samples, NULL,
                &capacity, &total);
            vws_store_i64(frame, total);
            return status;
        }

        case VWS_PROCESS_PCM: {
            if (frame == NULL || frame_len < 8) return VOCE_ERR_INVALID_ARGUMENT;
            int64_t capacity = vws_load_i64(frame);
            if (capacity < 0) return VOCE_ERR_INVALID_ARGUMENT;
            int64_t pcm_bytes = frame_len - 8 - capacity * VWS_RESULT_BYTES;
            if (pcm_bytes < 0 || pcm_bytes % 4 != 0) return VOCE_ERR_INVALID_ARGUMENT;
            int64_t samples = pcm_bytes / 4;
            VoceWindowResult *results = NULL;
            if (capacity > 0) {
                results = (VoceWindowResult *)malloc((size_t)capacity * sizeof(*results));
                if (results == NULL) return VOCE_ERR_IO;
            }
            int64_t written = capacity;
            int64_t total = 0;
            int32_t status = voce_engine_process_pcm(
                engine, (const float *)(frame + 8), samples, results, &written, &total);
            if (status == VOCE_OK && results != NULL) {
                memcpy(frame + 8 + pcm_bytes, results,
                       (size_t)written * sizeof(*results));
            }
            free(results);
            vws_store_i64(frame, total);
            return status;
        }

        case VWS_SERIALIZE_QUERY: {
            if (frame == NULL || frame_len < 8) return VOCE_ERR_INVALID_ARGUMENT;
            vws_store_i64(frame, 0);
            int64_t capacity = 0;
            int32_t status = voce_engine_serialize_profile_json(engine, NULL, &capacity);
            vws_store_i64(frame, capacity);
            return status;
        }

        case VWS_SERIALIZE_JSON: {
            if (frame == NULL || frame_len < 8) return VOCE_ERR_INVALID_ARGUMENT;
            int64_t capacity = vws_load_i64(frame);
            if (capacity < 0 || frame_len - 8 < capacity) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            int64_t written = capacity;
            int32_t status = voce_engine_serialize_profile_json(
                engine, (char *)(frame + 8), &written);
            vws_store_i64(frame, written);
            return status;
        }

        case VWS_DESERIALIZE_JSON:
            if (frame == NULL) return VOCE_ERR_INVALID_ARGUMENT;
            return voce_engine_deserialize_profile_json(
                engine, (const char *)frame, frame_len);

        default:
            return VOCE_ERR_UNSUPPORTED;
    }
}
