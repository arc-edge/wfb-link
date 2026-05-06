#pragma once

#include <stdint.h>

#ifndef AF_PACKET
#define AF_PACKET 17
#endif

#ifndef PF_PACKET
#define PF_PACKET AF_PACKET
#endif

#ifndef PACKET_QDISC_BYPASS
#define PACKET_QDISC_BYPASS 20
#endif

#ifndef SOL_PACKET
#define SOL_PACKET 263
#endif

#ifndef SO_MARK
#define SO_MARK 36
#endif

#ifndef SO_RXQ_OVFL
#define SO_RXQ_OVFL 40
#endif

#ifndef SIOCGIFINDEX
#define SIOCGIFINDEX 0x8933
#endif

#ifndef ifr_ifindex
#define ifr_ifindex ifr_metric
#endif

struct sockaddr_ll {
    uint16_t sll_family;
    uint16_t sll_protocol;
    int32_t sll_ifindex;
    uint16_t sll_hatype;
    uint8_t sll_pkttype;
    uint8_t sll_halen;
    uint8_t sll_addr[8];
};
