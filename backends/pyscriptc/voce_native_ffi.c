/* Extern FFI thunks binding pyscriptc's runtime ABI to the real voce_native C
 * ABI (native/voce_native.h, version 1).
 *
 * pyscriptc passes list[T] as a PsSeq pointer (f64 cells for float), str/bytes
 * as an (i64 len, char *data) pair plus an out PsBuf, and handles as opaque
 * pointers. Every thunk here borrows caller memory for the call only, which
 * matches the native header's ownership rules. Variable-size native outputs
 * use the documented query/required-capacity protocol: NULL data with
 * *len = 0, then a second call with a caller-owned buffer of that capacity.
 */

#include <ps_rt.h>

#include "../../native/voce_native.h"

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int64_t ffi_open_count = 0;
static int64_t ffi_close_count = 0;

int64_t ps_ffi_voce_open_count(void) { return ffi_open_count; }
int64_t ps_ffi_voce_close_count(void) { return ffi_close_count; }

/* Use the same /proc sources as native/src/metrics.rs so pyscriptc RSS is
 * directly comparable with the native probe. getrusage(RUSAGE_SELF).ru_maxrss
 * is inherited across execve on this host and reported ~498 MB at process
 * start, which was a measurement artifact rather than engine memory. */
int64_t ps_ffi_voce_rss_kb(void) {
    FILE *statm = fopen("/proc/self/statm", "r");
    if (statm == NULL) {
        return 0;
    }
    long resident_pages = 0;
    int fields = fscanf(statm, "%*ld %ld", &resident_pages);
    fclose(statm);
    if (fields != 1) {
        return 0;
    }
    return (int64_t)resident_pages * 4;
}

int64_t ps_ffi_voce_peak_rss_kb(void) {
    FILE *status = fopen("/proc/self/status", "r");
    if (status == NULL) {
        return 0;
    }
    char line[256];
    int64_t kib = 0;
    while (fgets(line, (int)sizeof(line), status) != NULL) {
        if (strncmp(line, "VmHWM:", 6) == 0) {
            kib = (int64_t)strtoll(line + 6, NULL, 10);
            break;
        }
    }
    fclose(status);
    return kib;
}

void *ps_ffi_voce_create(void) {
    VoceEngine *engine = voce_engine_create();
    if (engine != NULL) {
        ffi_open_count += 1;
    }
    return engine;
}

void ps_ffi_voce_close(void *engine) {
    if (engine != NULL) {
        ffi_close_count += 1;
    }
    voce_engine_destroy((VoceEngine *)engine);
}

int64_t ps_ffi_voce_abi_version(void) { return (int64_t)voce_abi_version(); }

/* Convert a pyscriptc f64 PsSeq into a freshly allocated f32 PCM buffer. */
static float *pcm_from_seq(PsSeq *seq, int64_t *count_out) {
    if (seq == NULL || seq->len < 0 || seq->elem != (int64_t)sizeof(double)) {
        return NULL;
    }
    float *pcm = (float *)malloc((size_t)(seq->len > 0 ? seq->len : 1) * sizeof(float));
    if (pcm == NULL) {
        return NULL;
    }
    const double *values = (const double *)seq->data;
    int64_t i = 0;
    while (i < seq->len) {
        pcm[i] = (float)values[i];
        i += 1;
    }
    *count_out = seq->len;
    return pcm;
}

int64_t ps_ffi_voce_load_models(void *engine, int64_t n, const char *path) {
    char *owned = (char *)malloc((size_t)n + 1);
    int64_t status;
    if (owned == NULL) {
        return VOCE_ERR_IO;
    }
    memcpy(owned, path, (size_t)n);
    owned[n] = 0;
    status = (int64_t)voce_engine_load_models((VoceEngine *)engine, owned);
    free(owned);
    return status;
}

int64_t ps_ffi_voce_set_threshold(void *engine, double threshold) {
    return (int64_t)voce_engine_set_threshold((VoceEngine *)engine, (float)threshold);
}

int64_t ps_ffi_voce_reset_enrollment(void *engine) {
    return (int64_t)voce_engine_reset_enrollment((VoceEngine *)engine);
}

int64_t ps_ffi_voce_enroll_pcm(void *engine, PsSeq *seq) {
    int64_t count = 0;
    float *pcm = pcm_from_seq(seq, &count);
    if (pcm == NULL) {
        return VOCE_ERR_NULL_ARGUMENT;
    }
    int64_t status = (int64_t)voce_engine_enroll_pcm((VoceEngine *)engine, pcm, count);
    free(pcm);
    return status;
}

/* Query protocol: embedding_dim writes a fixed out parameter; no query needed. */
int64_t ps_ffi_voce_embedding_dim(void *engine) {
    int64_t dim = 0;
    int64_t status = (int64_t)voce_engine_embedding_dim((VoceEngine *)engine, &dim);
    if (status != VOCE_OK) {
        return -status;
    }
    return dim;
}

/* finish_enrollment_capacity(engine) -> required f64 cell count. This is the
 * native NULL-data size query, surfaced so Python can allocate the exact
 * caller-owned output list before the real call. */
int64_t ps_ffi_voce_finish_enrollment_capacity(void *engine) {
    int64_t capacity = 0;
    int64_t status = (int64_t)voce_engine_finish_enrollment(
        (VoceEngine *)engine, NULL, &capacity);
    if (status != VOCE_OK) {
        return -status;
    }
    return capacity;
}

/* finish_enrollment(engine, out) -> status. `out` is the caller-sized f64
 * buffer from Python. The native size query happens first; a too-small output
 * list returns VOCE_ERR_BUFFER_TOO_SMALL without writing to it. */
int64_t ps_ffi_voce_finish_enrollment(void *engine, PsSeq *out) {
    int64_t capacity = 0;
    int64_t status = (int64_t)voce_engine_finish_enrollment((VoceEngine *)engine, NULL, &capacity);
    if (status != VOCE_OK) {
        return status;
    }
    if (out == NULL || out->len < capacity || out->elem != (int64_t)sizeof(double)) {
        return VOCE_ERR_BUFFER_TOO_SMALL;
    }
    float *embedding = (float *)malloc((size_t)capacity * sizeof(float));
    if (embedding == NULL) {
        return VOCE_ERR_IO;
    }
    int64_t written = capacity;
    status = (int64_t)voce_engine_finish_enrollment((VoceEngine *)engine, embedding, &written);
    if (status == VOCE_OK) {
        int64_t i = 0;
        while (i < written) {
            double value = (double)embedding[i];
            ps_seq_set(out, i, &value);
            i += 1;
        }
    }
    free(embedding);
    return status;
}

/* score_window(engine, pcm, out) -> status. The four output cells are
 * runtime_ms, similarity, speech_probability and decision, all as f64. */
int64_t ps_ffi_voce_score_window(void *engine, PsSeq *seq, PsSeq *out) {
    int64_t count = 0;
    float *pcm = pcm_from_seq(seq, &count);
    if (pcm == NULL) {
        return VOCE_ERR_NULL_ARGUMENT;
    }
    if (out == NULL || out->len < 4 || out->elem != (int64_t)sizeof(double)) {
        free(pcm);
        return VOCE_ERR_BUFFER_TOO_SMALL;
    }
    VoceWindowResult result;
    memset(&result, 0, sizeof(result));
    int64_t status = (int64_t)voce_engine_score_window(
        (VoceEngine *)engine, pcm, count, &result);
    free(pcm);
    if (status == VOCE_OK) {
        double values[4];
        values[0] = result.runtime_ms;
        values[1] = (double)result.similarity;
        values[2] = (double)result.speech_probability;
        values[3] = (double)result.decision;
        int64_t i = 0;
        while (i < 4) {
            ps_seq_set(out, i, &values[i]);
            i += 1;
        }
    }
    return status;
}

/* process_pcm_capacity(engine, n_samples) -> complete window count. This is
 * the native NULL-data size query, surfaced so Python allocates the exact
 * caller-sized output buffer. */
int64_t ps_ffi_voce_process_pcm_capacity(void *engine, int64_t n_samples) {
    int64_t capacity = 0;
    int64_t total = 0;
    int64_t status = (int64_t)voce_engine_process_pcm(
        (VoceEngine *)engine, NULL, n_samples, NULL, &capacity, &total);
    if (status != VOCE_OK) {
        return -status;
    }
    return total;
}

/* process_pcm(engine, pcm, out) -> status. `out` holds 4 f64 cells per window.
 * Python sizes it from process_pcm_capacity(); the native call gets capacity
 * len/4 and respects the same buffer-too-small protocol. */
int64_t ps_ffi_voce_process_pcm(void *engine, PsSeq *seq, PsSeq *out) {
    int64_t count = 0;
    float *pcm = pcm_from_seq(seq, &count);
    if (pcm == NULL) {
        return VOCE_ERR_NULL_ARGUMENT;
    }
    if (out == NULL || out->elem != (int64_t)sizeof(double) || (out->len % 4) != 0) {
        free(pcm);
        return VOCE_ERR_BUFFER_TOO_SMALL;
    }
    int64_t capacity = out->len / 4;
    int64_t total = 0;
    int64_t written = capacity;
    VoceWindowResult *results = NULL;
    if (capacity > 0) {
        results = (VoceWindowResult *)malloc((size_t)capacity * sizeof(VoceWindowResult));
        if (results == NULL) {
            free(pcm);
            return VOCE_ERR_IO;
        }
    }
    int64_t status = (int64_t)voce_engine_process_pcm(
        (VoceEngine *)engine, pcm, count, results, &written, &total);
    free(pcm);
    if (status == VOCE_OK) {
        int64_t i = 0;
        while (i < written) {
            double values[4];
            values[0] = results[i].runtime_ms;
            values[1] = (double)results[i].similarity;
            values[2] = (double)results[i].speech_probability;
            values[3] = (double)results[i].decision;
            int64_t cell = i * 4;
            int64_t k = 0;
            while (k < 4) {
                ps_seq_set(out, cell + k, &values[k]);
                k += 1;
            }
            i += 1;
        }
    }
    free(results);
    if (status == VOCE_OK) {
        return total;
    }
    return status;
}

/* JSON query returns bytes including the trailing NUL, per the native ABI. */
int64_t ps_ffi_voce_profile_json_capacity(void *engine) {
    int64_t capacity = 0;
    int64_t status = (int64_t)voce_engine_serialize_profile_json(
        (VoceEngine *)engine, NULL, &capacity);
    if (status != VOCE_OK) {
        return -status;
    }
    return capacity;
}

/* serialize_profile_json(engine) -> str: query, caller-owned malloc buffer,
 * then ps_buf_copy into the Python-owned string result. */
void ps_ffi_voce_serialize_profile_json(void *engine, PsBuf *out) {
    int64_t capacity = 0;
    int64_t status = (int64_t)voce_engine_serialize_profile_json(
        (VoceEngine *)engine, NULL, &capacity);
    if (status != VOCE_OK || capacity <= 0) {
        ps_buf_copy(out, "", 0);
        return;
    }
    char *buffer = (char *)malloc((size_t)capacity);
    if (buffer == NULL) {
        ps_buf_copy(out, "", 0);
        return;
    }
    int64_t written = capacity;
    status = (int64_t)voce_engine_serialize_profile_json(
        (VoceEngine *)engine, buffer, &written);
    if (status != VOCE_OK) {
        ps_buf_copy(out, "", 0);
    } else {
        ps_buf_copy(out, buffer, written);
    }
    free(buffer);
}

int64_t ps_ffi_voce_deserialize_profile_json(void *engine, int64_t n, const char *json) {
    return (int64_t)voce_engine_deserialize_profile_json(
        (VoceEngine *)engine, json, n);
}
