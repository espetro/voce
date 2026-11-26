/*
 * Reviewed narrow adapter over the voce_native C ABI (voce_native.h v1).
 * See voce_adapter.h for the exported contracts and per-op frame layouts.
 *
 * This is the only place in the repository that converts byte spans into
 * f32 element counts, owns the session table, and implements the native
 * capacity-query protocol. Language backends bind the three exported
 * functions and never call voce_native.h directly.
 */

#include "voce_adapter.h"
#include "voce_native.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

_Static_assert(sizeof(VoceWindowResult) == VOCE_ADAPTER_WINDOW_RESULT_BYTES,
               "VoceWindowResult frame layout changed");

static VoceEngine *sessions[VOCE_ADAPTER_MAX_SESSIONS];

static VoceEngine *session_engine(int32_t session) {
    if (session < 1 || session > VOCE_ADAPTER_MAX_SESSIONS) {
        return NULL;
    }
    return sessions[session - 1];
}

static int64_t load_i64(const uint8_t *p) {
    int64_t value;
    memcpy(&value, p, sizeof(value));
    return value;
}

static void store_i64(uint8_t *p, int64_t value) {
    memcpy(p, &value, sizeof(value));
}

static float load_f32(const uint8_t *p) {
    float value;
    memcpy(&value, p, sizeof(value));
    return value;
}

/* Copy byte-span PCM into an aligned f32 buffer. No input is retained. */
static float *pcm_from_frame(const uint8_t *bytes, int64_t n_samples) {
    float *pcm;
    if (n_samples <= 0) {
        return NULL;
    }
    pcm = (float *)malloc((size_t)n_samples * sizeof(*pcm));
    if (pcm == NULL) {
        return NULL;
    }
    memcpy(pcm, bytes, (size_t)n_samples * sizeof(*pcm));
    return pcm;
}

int32_t voce_adapter_create(void) {
    int i;
    for (i = 0; i < VOCE_ADAPTER_MAX_SESSIONS; i++) {
        if (sessions[i] == NULL) {
            sessions[i] = voce_engine_create();
            if (sessions[i] == NULL) {
                return VOCE_ERR_IO;
            }
            return (int32_t)(i + 1);
        }
    }
    return VOCE_ERR_UNSUPPORTED;
}

void voce_adapter_destroy(int32_t session) {
    VoceEngine *engine = session_engine(session);
    if (engine != NULL) {
        voce_engine_destroy(engine);
        sessions[session - 1] = NULL;
    }
}

int32_t voce_adapter_frame(int32_t session, int32_t op,
                           uint8_t *frame, int64_t frame_len) {
    VoceEngine *engine = session_engine(session);
    int32_t status;

    if (engine == NULL) {
        return VOCE_ERR_INVALID_STATE;
    }
    if (frame_len < 0 || (frame == NULL && frame_len > 0)) {
        return VOCE_ERR_INVALID_ARGUMENT;
    }

    switch (op) {
        case VOCE_ADAPTER_LOAD_MODELS:
            if (frame == NULL || frame_len < 1 || frame[frame_len - 1] != 0) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            return voce_engine_load_models(engine, (const char *)frame);

        case VOCE_ADAPTER_SET_THRESHOLD:
            if (frame == NULL || frame_len < 4) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            return voce_engine_set_threshold(engine, load_f32(frame));

        case VOCE_ADAPTER_RESET_ENROLLMENT:
            return voce_engine_reset_enrollment(engine);

        case VOCE_ADAPTER_ENROLL_PCM: {
            int64_t samples;
            float *pcm;
            if (frame_len % 4 != 0) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            samples = frame_len / 4;
            if (samples == 0) {
                return voce_engine_enroll_pcm(engine, NULL, 0);
            }
            pcm = pcm_from_frame(frame, samples);
            if (pcm == NULL) {
                return VOCE_ERR_IO;
            }
            status = voce_engine_enroll_pcm(engine, pcm, samples);
            free(pcm);
            return status;
        }

        case VOCE_ADAPTER_FINISH_QUERY: {
            int64_t capacity = 0;
            if (frame == NULL || frame_len < VOCE_ADAPTER_I64_PREFIX_BYTES) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            status = voce_engine_finish_enrollment(engine, NULL, &capacity);
            store_i64(frame, capacity);
            return status;
        }

        case VOCE_ADAPTER_FINISH_ENROLLMENT: {
            int64_t capacity;
            int64_t written;
            float *embedding;
            if (frame == NULL || frame_len < VOCE_ADAPTER_I64_PREFIX_BYTES) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            capacity = load_i64(frame);
            if (capacity < 0 || capacity > (frame_len - 8) / 4) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            embedding = capacity > 0
                ? (float *)malloc((size_t)capacity * sizeof(*embedding))
                : NULL;
            if (capacity > 0 && embedding == NULL) {
                return VOCE_ERR_IO;
            }
            written = capacity;
            status = voce_engine_finish_enrollment(engine, embedding, &written);
            if (status == VOCE_OK && embedding != NULL) {
                memcpy(frame + 8, embedding, (size_t)written * sizeof(*embedding));
            }
            free(embedding);
            store_i64(frame, written);
            return status;
        }

        case VOCE_ADAPTER_EMBEDDING_DIM: {
            int64_t dim = 0;
            if (frame == NULL || frame_len < VOCE_ADAPTER_I64_PREFIX_BYTES) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            status = voce_engine_embedding_dim(engine, &dim);
            store_i64(frame, dim);
            return status;
        }

        case VOCE_ADAPTER_SCORE_WINDOW: {
            int64_t samples;
            float *pcm;
            VoceWindowResult result;
            if (frame == NULL || frame_len < VOCE_ADAPTER_WINDOW_RESULT_BYTES ||
                (frame_len - VOCE_ADAPTER_WINDOW_RESULT_BYTES) % 4 != 0) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            samples = (frame_len - VOCE_ADAPTER_WINDOW_RESULT_BYTES) / 4;
            pcm = pcm_from_frame(frame, samples);
            if (pcm == NULL && samples > 0) {
                return VOCE_ERR_IO;
            }
            memset(&result, 0, sizeof(result));
            status = voce_engine_score_window(engine, pcm, samples, &result);
            free(pcm);
            if (status == VOCE_OK) {
                memcpy(frame + frame_len - VOCE_ADAPTER_WINDOW_RESULT_BYTES,
                       &result, sizeof(result));
            }
            return status;
        }

        case VOCE_ADAPTER_PROCESS_QUERY: {
            int64_t samples;
            int64_t capacity = 0;
            int64_t total = 0;
            float *pcm;
            if (frame == NULL || frame_len < VOCE_ADAPTER_I64_PREFIX_BYTES ||
                (frame_len - VOCE_ADAPTER_I64_PREFIX_BYTES) % 4 != 0) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            samples = (frame_len - VOCE_ADAPTER_I64_PREFIX_BYTES) / 4;
            pcm = pcm_from_frame(frame + 8, samples);
            if (pcm == NULL && samples > 0) {
                return VOCE_ERR_IO;
            }
            status = voce_engine_process_pcm(engine, pcm, samples, NULL,
                                             &capacity, &total);
            free(pcm);
            store_i64(frame, total);
            return status;
        }

        case VOCE_ADAPTER_PROCESS_PCM: {
            int64_t capacity;
            int64_t pcm_bytes;
            int64_t samples;
            int64_t written;
            int64_t total = 0;
            float *pcm;
            VoceWindowResult *results;
            if (frame == NULL || frame_len < VOCE_ADAPTER_I64_PREFIX_BYTES) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            capacity = load_i64(frame);
            if (capacity < 0 ||
                capacity > (frame_len - VOCE_ADAPTER_I64_PREFIX_BYTES) /
                               VOCE_ADAPTER_WINDOW_RESULT_BYTES) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            pcm_bytes = frame_len - VOCE_ADAPTER_I64_PREFIX_BYTES -
                        capacity * VOCE_ADAPTER_WINDOW_RESULT_BYTES;
            if (pcm_bytes < 0 || pcm_bytes % 4 != 0) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            samples = pcm_bytes / 4;
            pcm = pcm_from_frame(frame + 8, samples);
            if (pcm == NULL && samples > 0) {
                return VOCE_ERR_IO;
            }
            results = capacity > 0
                ? (VoceWindowResult *)malloc((size_t)capacity * sizeof(*results))
                : NULL;
            if (capacity > 0 && results == NULL) {
                free(pcm);
                return VOCE_ERR_IO;
            }
            written = capacity;
            status = voce_engine_process_pcm(engine, pcm, samples, results,
                                             &written, &total);
            free(pcm);
            if (status == VOCE_OK && results != NULL) {
                memcpy(frame + 8 + pcm_bytes, results,
                       (size_t)written * sizeof(*results));
            }
            free(results);
            store_i64(frame, written);
            return status;
        }

        case VOCE_ADAPTER_SERIALIZE_QUERY: {
            int64_t capacity = 0;
            if (frame == NULL || frame_len < VOCE_ADAPTER_I64_PREFIX_BYTES) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            status = voce_engine_serialize_profile_json(engine, NULL, &capacity);
            store_i64(frame, capacity);
            return status;
        }

        case VOCE_ADAPTER_SERIALIZE_PROFILE_JSON: {
            int64_t capacity;
            int64_t written;
            char *json;
            if (frame == NULL || frame_len < VOCE_ADAPTER_I64_PREFIX_BYTES) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            capacity = load_i64(frame);
            if (capacity < 0 || capacity > frame_len - 8) {
                return VOCE_ERR_INVALID_ARGUMENT;
            }
            json = (char *)malloc((size_t)capacity + 1);
            if (json == NULL) {
                return VOCE_ERR_IO;
            }
            written = capacity;
            status = voce_engine_serialize_profile_json(engine, json, &written);
            if (status == VOCE_OK) {
                memcpy(frame + 8, json, (size_t)written);
            }
            free(json);
            store_i64(frame, written);
            return status;
        }

        case VOCE_ADAPTER_DESERIALIZE_PROFILE_JSON:
            return voce_engine_deserialize_profile_json(
                engine, frame != NULL ? (const char *)frame : "", frame_len);

        default:
            return VOCE_ERR_UNSUPPORTED;
    }
}
