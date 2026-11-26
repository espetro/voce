#ifndef VOCE_ADAPTER_H
#define VOCE_ADAPTER_H

/*
 * Voce native adapter, adapter ABI version 1.
 *
 * This is the single reviewed compatibility surface between the voce_native
 * C ABI (voce_native.h, ABI version 1) and language backends whose FFI cannot
 * express `int64_t *in_out_*` parameters (scriptc mutable-bytes lowers to a
 * (ptr, byte_length) pair; pyscriptc PsSeq cells are f64). Backends bind only
 * the three functions below. All variable-size outputs are length-prefixed
 * byte frames; the only fixed-width struct is the 24-byte window result.
 *
 * Contracts
 * ---------
 * - voce_adapter_create returns a positive session id, or a negative
 *   VoceStatus code on failure. A session id owns exactly one VoceEngine.
 * - voce_adapter_destroy consumes a session id exactly once. Passing an
 *   already destroyed, foreign, or out-of-range id is a no-op. Ids may be
 *   reused after destroy; callers must not keep stale ids.
 * - `frame` is caller-owned and borrowed for the duration of one call. The
 *   adapter never stores it, never allocates a result buffer for the caller,
 *   and never returns a pointer. All multi-byte fields are little-endian.
 * - `frame_len` is a byte count, never an element count. The adapter is the
 *   only place that converts byte lengths to f32 element counts or to
 *   VoceWindowResult element counts.
 * - Prefix fields occupy the first 8 bytes of a frame and are int64. On a
 *   capacity-carrying write, the caller stores the capacity there; on return
 *   it holds the number of elements/bytes written. On a too-small buffer
 *   (-4), it holds the required capacity and no payload is written.
 * - Query ops ignore an incoming prefix and overwrite it with the required
 *   capacity. A query never mutates engine state, except that a successful
 *   finish write after the query finalizes enrollment as documented in
 *   voce_native.h.
 * - No callbacks cross this ABI. No function is variadic. There are no C++
 *   types, platform includes, or allocator APIs in this header.
 * - Sessions are process-global and not thread-safe. Serialize all calls or
 *   use a separate process per backend.
 * - Status codes are VoceStatus from voce_native.h, plus
 *   VOCE_ERR_INVALID_STATE (-3) for a dead session and
 *   VOCE_ERR_UNSUPPORTED (-7) for an unknown op or malformed frame.
 */

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define VOCE_ADAPTER_ABI_VERSION 1
#define VOCE_ADAPTER_MAX_SESSIONS 16
#define VOCE_ADAPTER_I64_PREFIX_BYTES 8
#define VOCE_ADAPTER_WINDOW_RESULT_BYTES 24
#define VOCE_ADAPTER_EMBEDDING_F32 256

/* Operation codes for voce_adapter_frame. */
enum VoceAdapterOp {
    VOCE_ADAPTER_LOAD_MODELS = 1,
    VOCE_ADAPTER_SET_THRESHOLD = 2,
    VOCE_ADAPTER_RESET_ENROLLMENT = 3,
    VOCE_ADAPTER_ENROLL_PCM = 4,
    VOCE_ADAPTER_FINISH_QUERY = 5,
    VOCE_ADAPTER_FINISH_ENROLLMENT = 6,
    VOCE_ADAPTER_EMBEDDING_DIM = 7,
    VOCE_ADAPTER_SCORE_WINDOW = 8,
    VOCE_ADAPTER_PROCESS_QUERY = 9,
    VOCE_ADAPTER_PROCESS_PCM = 10,
    VOCE_ADAPTER_SERIALIZE_QUERY = 11,
    VOCE_ADAPTER_SERIALIZE_PROFILE_JSON = 12,
    VOCE_ADAPTER_DESERIALIZE_PROFILE_JSON = 13
};

/* Returns a session id >= 1, or a negative VoceStatus code. */
int32_t voce_adapter_create(void);

/* Consumes a session id. NULL/dead/invalid ids are ignored. */
void voce_adapter_destroy(int32_t session);

/*
 * Executes one operation on a live session. Returns VOCE_OK (0) or a negative
 * VoceStatus code. `frame` may be NULL only for ops whose payload is empty.
 *
 * Per-op frame layouts (little-endian):
 *   1 LOAD_MODELS: path bytes including a trailing NUL. Requires len >= 1.
 *   2 SET_THRESHOLD: exactly 4 bytes, f32 in [0, 1].
 *   3 RESET_ENROLLMENT: empty frame.
 *   4 ENROLL_PCM: f32 PCM bytes; frame_len must be a multiple of 4.
 *   5 FINISH_QUERY: 8-byte prefix; on success holds required f32 count.
 *   6 FINISH_ENROLLMENT: [capacity i64][f32 embedding * capacity]. On success
 *     the prefix holds the f32 count written.
 *   7 EMBEDDING_DIM: 8-byte prefix; on success holds the i64 dimension.
 *   8 SCORE_WINDOW: [f32 PCM bytes][24-byte VoceWindowResult]. The result is
 *     the trailing 24 bytes: f64 runtime_ms, f32 similarity,
 *     f32 speech_probability, i32 decision. Requires at least 24 bytes and a
 *     frame_len divisible by 4 after subtracting the result.
 *   9 PROCESS_QUERY: [8-byte prefix][f32 PCM bytes]. Uses the real PCM for
 *     the native call (a NULL PCM query is invalid) and NULL results. On
 *     success the prefix holds the complete-window count.
 *  10 PROCESS_PCM: [capacity i64][f32 PCM bytes][24-byte result * capacity].
 *     On success the prefix holds results written; on -4 it holds the
 *     required result count. The result array starts at
 *     8 + (frame_len - 8 - capacity * 24).
 *  11 SERIALIZE_QUERY: 8-byte prefix; on success holds required JSON bytes
 *     including the trailing NUL.
 *  12 SERIALIZE_PROFILE_JSON: [capacity i64][JSON bytes]. Capacity includes
 *     room for the trailing NUL. On success the prefix holds bytes written,
 *     excluding the NUL.
 *  13 DESERIALIZE_PROFILE_JSON: JSON bytes without a trailing NUL; the
 *     explicit byte length is passed to the native ABI.
 */
int32_t voce_adapter_frame(int32_t session, int32_t op,
                           uint8_t *frame, int64_t frame_len);

#ifdef __cplusplus
}
#endif

#endif /* VOCE_ADAPTER_H */
