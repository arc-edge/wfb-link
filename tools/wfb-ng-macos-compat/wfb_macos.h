#pragma once

#include <sys/socket.h>
#include <time.h>

#ifndef SO_MARK
#define SO_MARK 36
#endif

#ifndef SO_RXQ_OVFL
#define SO_RXQ_OVFL 40
#endif

static inline int wfb_macos_setsockopt(int socket, int level, int option_name,
                                       const void *option_value,
                                       socklen_t option_len) {
    if (level == SOL_SOCKET && option_name == SO_RXQ_OVFL) {
        return 0;
    }
    if (level == SOL_SOCKET && option_name == SO_MARK) {
        return 0;
    }
    return setsockopt(socket, level, option_name, option_value, option_len);
}

static inline int wfb_macos_clock_nanosleep(clockid_t clock_id, int flags,
                                            const struct timespec *request,
                                            struct timespec *remain) {
    (void)clock_id;
    (void)flags;
    return nanosleep(request, remain);
}

#define setsockopt wfb_macos_setsockopt
#define clock_nanosleep wfb_macos_clock_nanosleep
