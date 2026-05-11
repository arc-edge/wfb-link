#pragma once

/*
 * Minimal libpcap compatibility surface for Android wfb_rx builds.
 *
 * wfb_rx links the same object file for raw-interface RX and aggregator mode.
 * The Android managed-stream path only uses aggregator mode (-a), so the pcap
 * entry points are intentionally inert stubs that fail if raw-interface mode is
 * selected.
 */

#include <sys/time.h>

#ifdef __cplusplus
extern "C" {
#endif

#define PCAP_ERRBUF_SIZE 256
#define DLT_IEEE802_11_RADIO 127

typedef struct pcap pcap_t;

struct pcap_pkthdr {
    struct timeval ts;
    unsigned int caplen;
    unsigned int len;
};

struct bpf_program {
    unsigned int bf_len;
    void *bf_insns;
};

pcap_t *pcap_create(const char *source, char *errbuf);
int pcap_set_buffer_size(pcap_t *p, int buffer_size);
int pcap_set_snaplen(pcap_t *p, int snaplen);
int pcap_set_promisc(pcap_t *p, int promisc);
int pcap_set_timeout(pcap_t *p, int timeout_ms);
int pcap_set_immediate_mode(pcap_t *p, int immediate_mode);
int pcap_activate(pcap_t *p);
int pcap_setnonblock(pcap_t *p, int nonblock, char *errbuf);
int pcap_datalink(pcap_t *p);
int pcap_compile(pcap_t *p, struct bpf_program *fp, const char *str, int optimize, unsigned int netmask);
int pcap_setfilter(pcap_t *p, struct bpf_program *fp);
void pcap_freecode(struct bpf_program *fp);
int pcap_get_selectable_fd(pcap_t *p);
void pcap_close(pcap_t *p);
const unsigned char *pcap_next(pcap_t *p, struct pcap_pkthdr *h);
const char *pcap_geterr(pcap_t *p);

#ifdef __cplusplus
}
#endif
