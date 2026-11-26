/* C-level reproducer for both backend consumption shapes.
 *
 * Shape A mirrors scriptc: one caller-owned mutable byte frame per call.
 * Shape B mirrors pyscriptc: f64 cells are converted to a byte frame, the
 * adapter mutates it, and bytes are copied back. Both shapes bind only the
 * three adapter symbols and run the same probe flow.
 */

#include "../../native/voce_adapter.h"
#include "../../native/voce_native.h"

#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(expr) do { \
    int32_t code_ = (expr); \
    if (code_ != VOCE_OK) { \
        fprintf(stderr, "failed %s: %d\n", #expr, code_); \
        exit(1); \
    } \
} while (0)

static int64_t load_i64(const uint8_t *p) {
    int64_t value;
    memcpy(&value, p, sizeof(value));
    return value;
}

static void store_i64(uint8_t *p, int64_t value) {
    memcpy(p, &value, sizeof(value));
}

/* scriptc shape: pass the frame straight through. */
static int32_t call_scriptc(int32_t session, int32_t op,
                            uint8_t *frame, int64_t len) {
    return voce_adapter_frame(session, op, frame, len);
}

/* pyscriptc shape: f64 cell list -> byte frame -> f64 cell list. */
static int32_t call_pyscriptc(int32_t session, int32_t op,
                              double *cells, int64_t n) {
    uint8_t *frame;
    int32_t status;
    int64_t i;
    frame = (uint8_t *)malloc((size_t)(n > 0 ? n : 1));
    if (frame == NULL) {
        return VOCE_ERR_IO;
    }
    for (i = 0; i < n; i++) {
        frame[i] = (uint8_t)cells[i];
    }
    status = voce_adapter_frame(session, op, frame, n);
    for (i = 0; i < n; i++) {
        cells[i] = (double)frame[i];
    }
    free(frame);
    return status;
}

static void push_pcm(uint8_t *frame, int64_t count) {
    float value = 0.25f;
    int64_t i;
    for (i = 0; i < count; i++) {
        memcpy(frame + i * 4, &value, sizeof(value));
    }
}

static void push_tone(uint8_t *frame, int64_t count) {
    int64_t i;
    for (i = 0; i < count; i++) {
        float value = (float)(sin(2.0 * 3.141592653589793 * 220.0 * i / 16000.0) * 0.2);
        memcpy(frame + i * 4, &value, sizeof(value));
    }
}

int main(void) {
    const char *path = "mock://builtin";
    int32_t session = voce_adapter_create();
    int32_t restored;
    int64_t dim = 0;
    int64_t windows = 0;
    int64_t json_capacity = 0;
    int64_t json_written = 0;
    int8_t json[8192];
    uint8_t *enroll = NULL;
    uint8_t *embedding;
    uint8_t *score = NULL;
    uint8_t *batch;
    uint8_t *results;
    double cells[16];

    if (session < 1) {
        return 2;
    }
    /* pyscriptc shape: a path encoded as f64 cells, then copied to bytes. */
    memset(cells, 0, sizeof(cells));
    strcpy((char *)json, path);
    for (size_t i = 0; i < strlen(path) + 1; i++) {
        cells[i] = (double)(uint8_t)json[i];
    }
    CHECK(call_pyscriptc(session, VOCE_ADAPTER_LOAD_MODELS, cells,
                         (int64_t)strlen(path) + 1));
    CHECK(call_scriptc(session, VOCE_ADAPTER_RESET_ENROLLMENT, NULL, 0));
    enroll = (uint8_t *)malloc((size_t)VOCE_WINDOW_SAMPLES * 4);
    if (enroll == NULL) {
        return VOCE_ERR_IO;
    }
    push_tone(enroll, VOCE_WINDOW_SAMPLES);
    CHECK(call_scriptc(session, VOCE_ADAPTER_ENROLL_PCM, enroll,
                       (int64_t)VOCE_WINDOW_SAMPLES * 4));
    free(enroll);

    embedding = (uint8_t *)calloc(1, 8 + VOCE_EMBEDDING_DIM * 4);
    store_i64(embedding, 0);
    CHECK(call_scriptc(session, VOCE_ADAPTER_FINISH_QUERY, embedding, 8));
    dim = load_i64(embedding);
    store_i64(embedding, dim);
    CHECK(call_scriptc(session, VOCE_ADAPTER_FINISH_ENROLLMENT, embedding,
                       8 + dim * 4));

    score = (uint8_t *)realloc(score, VOCE_WINDOW_SAMPLES * 4 + 24);
    push_pcm(score, VOCE_WINDOW_SAMPLES);
    CHECK(call_scriptc(session, VOCE_ADAPTER_SCORE_WINDOW, score,
                       VOCE_WINDOW_SAMPLES * 4 + 24));

    batch = (uint8_t *)calloc(1, 8 + (VOCE_WINDOW_SAMPLES + VOCE_HOP_SAMPLES) * 4);
    store_i64(batch, 0);
    push_pcm(batch + 8, VOCE_WINDOW_SAMPLES + VOCE_HOP_SAMPLES);
    CHECK(call_scriptc(session, VOCE_ADAPTER_PROCESS_QUERY, batch,
                       8 + (VOCE_WINDOW_SAMPLES + VOCE_HOP_SAMPLES) * 4));
    windows = load_i64(batch);

    results = (uint8_t *)calloc(1, 8 + (VOCE_WINDOW_SAMPLES + VOCE_HOP_SAMPLES) * 4 +
                                   (size_t)windows * 24);
    store_i64(results, windows);
    push_pcm(results + 8, VOCE_WINDOW_SAMPLES + VOCE_HOP_SAMPLES);
    CHECK(call_scriptc(session, VOCE_ADAPTER_PROCESS_PCM, results,
                       8 + (VOCE_WINDOW_SAMPLES + VOCE_HOP_SAMPLES) * 4 +
                           windows * 24));

    store_i64((uint8_t *)json, 0);
    CHECK(call_scriptc(session, VOCE_ADAPTER_SERIALIZE_QUERY, (uint8_t *)json, 8));
    json_capacity = load_i64((uint8_t *)json);
    store_i64((uint8_t *)json, json_capacity);
    CHECK(call_scriptc(session, VOCE_ADAPTER_SERIALIZE_PROFILE_JSON,
                       (uint8_t *)json, 8 + json_capacity));
    json_written = load_i64((uint8_t *)json);

    restored = voce_adapter_create();
    CHECK(call_scriptc(restored, VOCE_ADAPTER_LOAD_MODELS, (uint8_t *)path,
                       (int64_t)strlen(path) + 1));
    CHECK(call_scriptc(restored, VOCE_ADAPTER_DESERIALIZE_PROFILE_JSON,
                       (uint8_t *)json + 8, json_written));
    voce_adapter_destroy(restored);
    voce_adapter_destroy(session);

    printf("adapter C reproducer: dim=%lld windows=%lld json=%lld\n",
           (long long)dim, (long long)windows, (long long)json_written);
    free(embedding);
    free(score);
    free(batch);
    free(results);
    return 0;
}
