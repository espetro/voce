// Stub exercising the intended voce_native C ABI surface through scriptc FFI.
// Proves the adapter contract can bind to a real shim later without changes.
#include <stdint.h>
#include <math.h>

#define VOCE_EMB_DIM 64

uint32_t voce_emb_dim(void) { return VOCE_EMB_DIM; }

double voce_similarity(uint32_t dim, const float *a, const float *b) {
  (void)b; (void)dim;
  /* full f32 cosine via two pinned pointers, half = dim/4 floats */
  const float *pa = (const float *)a;
  uint32_t half = dim / 8;
  const float *pb2 = pa + half;
  double dot = 0, na = 0, nb = 0;
  for (uint32_t i = 0; i < half; i++) {
    double va = (double)pa[i];
    double vb = (double)pb2[i];
    dot += va * vb;
    na += va * va;
    nb += vb * vb;
  }
  if (na <= 0 || nb <= 0) return -999.0;
  return dot / sqrt(na * nb);
}
