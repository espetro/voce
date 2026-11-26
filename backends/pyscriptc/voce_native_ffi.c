/* Extern FFI thunks for the pyscriptc backend spike.
 *
 * Today this only exposes peak RSS through getrusage. The future
 * voce_native inference surface is documented in voce_native.toml and is
 * deliberately absent here: this spike uses the Python mock adapter.
 */

#include <stdint.h>
#include <sys/resource.h>

int64_t ps_ffi_voce_rss_kb(void) {
    struct rusage ru;
    getrusage(RUSAGE_SELF, &ru);
    return (int64_t)ru.ru_maxrss;
}
