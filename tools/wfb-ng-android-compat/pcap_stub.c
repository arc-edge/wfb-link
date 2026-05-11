#include "pcap.h"

#include <stdlib.h>
#include <string.h>

struct pcap {
    char error[PCAP_ERRBUF_SIZE];
};

static void set_error(pcap_t *p, const char *message)
{
    if (p) {
        strncpy(p->error, message, sizeof(p->error) - 1);
        p->error[sizeof(p->error) - 1] = '\0';
    }
}

pcap_t *pcap_create(const char *source, char *errbuf)
{
    (void)source;
    pcap_t *p = calloc(1, sizeof(*p));
    if (!p) {
        if (errbuf) {
            strncpy(errbuf, "pcap stub allocation failed", PCAP_ERRBUF_SIZE - 1);
            errbuf[PCAP_ERRBUF_SIZE - 1] = '\0';
        }
        return NULL;
    }
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return p;
}

int pcap_set_buffer_size(pcap_t *p, int buffer_size)
{
    (void)buffer_size;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_set_snaplen(pcap_t *p, int snaplen)
{
    (void)snaplen;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_set_promisc(pcap_t *p, int promisc)
{
    (void)promisc;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_set_timeout(pcap_t *p, int timeout_ms)
{
    (void)timeout_ms;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_set_immediate_mode(pcap_t *p, int immediate_mode)
{
    (void)immediate_mode;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_activate(pcap_t *p)
{
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_setnonblock(pcap_t *p, int nonblock, char *errbuf)
{
    (void)nonblock;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    if (errbuf) {
        strncpy(errbuf, pcap_geterr(p), PCAP_ERRBUF_SIZE - 1);
        errbuf[PCAP_ERRBUF_SIZE - 1] = '\0';
    }
    return -1;
}

int pcap_datalink(pcap_t *p)
{
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_compile(pcap_t *p, struct bpf_program *fp, const char *str, int optimize, unsigned int netmask)
{
    (void)fp;
    (void)str;
    (void)optimize;
    (void)netmask;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

int pcap_setfilter(pcap_t *p, struct bpf_program *fp)
{
    (void)fp;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

void pcap_freecode(struct bpf_program *fp)
{
    (void)fp;
}

int pcap_get_selectable_fd(pcap_t *p)
{
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return -1;
}

void pcap_close(pcap_t *p)
{
    free(p);
}

const unsigned char *pcap_next(pcap_t *p, struct pcap_pkthdr *h)
{
    (void)h;
    set_error(p, "pcap raw-interface mode is unavailable in the Android codec helper");
    return NULL;
}

const char *pcap_geterr(pcap_t *p)
{
    if (!p) {
        return "pcap raw-interface mode is unavailable in the Android codec helper";
    }
    return p->error[0] ? p->error : "pcap raw-interface mode is unavailable in the Android codec helper";
}
