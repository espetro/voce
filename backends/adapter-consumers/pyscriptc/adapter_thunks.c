/* pyscriptc extern thunks for the reviewed native adapter.
 *
 * pyscriptc binds only three functions. A Python list[int] carrying frame
 * bytes is the sole data shape: the thunk copies those bytes into a temporary
 * aligned buffer, calls the adapter with an explicit op code, and copies the
 * mutated frame bytes back. All native length handling stays in the adapter.
 */

#include <ps_rt.h>

#include "../../../native/voce_adapter.h"
#include "../../../native/voce_native.h"

#include <stdint.h>
#include <stdlib.h>
#include <string.h>

int64_t ps_ffi_va_create(void) {
    return (int64_t)voce_adapter_create();
}

int64_t ps_ffi_va_destroy(int64_t session) {
    voce_adapter_destroy((int32_t)session);
    return 0;
}

int64_t ps_ffi_va_call(int64_t session, int64_t op, PsSeq *frame) {
    uint8_t *bytes;
    int32_t status;
    int64_t i;
    int64_t *cells;

    if (frame == NULL || frame->len < 0 || frame->elem != (int64_t)sizeof(int64_t)) {
        return VOCE_ERR_INVALID_ARGUMENT;
    }
    bytes = (uint8_t *)malloc((size_t)(frame->len > 0 ? frame->len : 1));
    if (bytes == NULL) {
        return VOCE_ERR_IO;
    }
    cells = (int64_t *)frame->data;
    for (i = 0; i < frame->len; i++) {
        bytes[i] = (uint8_t)cells[i];
    }
    status = voce_adapter_frame((int32_t)session, (int32_t)op, bytes, frame->len);
    for (i = 0; i < frame->len; i++) {
        cells[i] = (int64_t)bytes[i];
    }
    free(bytes);
    return (int64_t)status;
}
