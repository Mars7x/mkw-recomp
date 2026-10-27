/* Project build configuration for the unmodified BSD SoftFloat sources.
 * The diagnostic target is little-endian GCC/Clang with 64-bit integers. */
#define LITTLEENDIAN 1
#define INLINE static inline
#define THREAD_LOCAL _Thread_local
#define SOFTFLOAT_BUILTIN_CLZ 1
#define SOFTFLOAT_INTRINSIC_INT128 1
#include "opts-GCC.h"
