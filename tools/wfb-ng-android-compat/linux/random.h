#pragma once

#include <sys/ioctl.h>

#ifndef RNDGETENTCNT
#define RNDGETENTCNT _IOR('R', 0x00, int)
#endif
