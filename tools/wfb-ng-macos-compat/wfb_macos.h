#pragma once

#include <time.h>

#ifndef SO_MARK
#define SO_MARK 36
#endif

#ifndef SO_RXQ_OVFL
#define SO_RXQ_OVFL 40
#endif

static inline int wfb_macos_clock_nanosleep(clockid_t clock_id, int flags,
                                            const struct timespec *request,
                                            struct timespec *remain) {
    (void)clock_id;
    (void)flags;
    return nanosleep(request, remain);
}

#define clock_nanosleep wfb_macos_clock_nanosleep
