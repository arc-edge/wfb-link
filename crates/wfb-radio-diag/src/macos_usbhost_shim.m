#import <Foundation/Foundation.h>
#import <IOKit/IOKitLib.h>
#import <IOUSBHost/IOUSBHost.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct WfbMacosUsbHost {
    void *device;
} WfbMacosUsbHost;

static void set_error(char *error, size_t error_len, NSString *message) {
    if (!error || error_len == 0) {
        return;
    }
    const char *utf8 = message ? message.UTF8String : "unknown error";
    if (!utf8) {
        utf8 = "unknown error";
    }
    snprintf(error, error_len, "%s", utf8);
}

static uint16_t number_property(io_service_t service, CFStringRef key) {
    uint16_t out = 0xffff;
    CFTypeRef value = IORegistryEntryCreateCFProperty(service, key, kCFAllocatorDefault, 0);
    if (value && CFGetTypeID(value) == CFNumberGetTypeID()) {
        int raw = 0;
        if (CFNumberGetValue((CFNumberRef)value, kCFNumberIntType, &raw) && raw >= 0 && raw <= 0xffff) {
            out = (uint16_t)raw;
        }
    }
    if (value) {
        CFRelease(value);
    }
    return out;
}

int wfb_macos_usbhost_open(uint16_t wanted_vid,
                           uint16_t wanted_pid,
                           WfbMacosUsbHost **out_host,
                           char *error,
                           size_t error_len) {
    @autoreleasepool {
        if (!out_host) {
            set_error(error, error_len, @"out_host was null");
            return -1;
        }
        *out_host = NULL;

        io_iterator_t iterator = IO_OBJECT_NULL;
        kern_return_t kr = IORegistryCreateIterator(kIOMainPortDefault,
                                                    kIOUSBPlane,
                                                    kIORegistryIterateRecursively,
                                                    &iterator);
        if (kr != KERN_SUCCESS) {
            set_error(error, error_len, [NSString stringWithFormat:@"IORegistryCreateIterator failed: 0x%08x", kr]);
            return -1;
        }

        io_service_t service = IO_OBJECT_NULL;
        while ((service = IOIteratorNext(iterator)) != IO_OBJECT_NULL) {
            if (!IOObjectConformsTo(service, "IOUSBHostDevice")) {
                IOObjectRelease(service);
                continue;
            }

            uint16_t vid = number_property(service, CFSTR("idVendor"));
            uint16_t pid = number_property(service, CFSTR("idProduct"));
            if (vid != wanted_vid || pid != wanted_pid) {
                IOObjectRelease(service);
                continue;
            }

            NSError *open_error = nil;
            IOUSBHostDevice *device = [[IOUSBHostDevice alloc] initWithIOService:service
                                                                            queue:nil
                                                                            error:&open_error
                                                                  interestHandler:nil];
            IOObjectRelease(service);
            IOObjectRelease(iterator);

            if (!device) {
                set_error(error, error_len, [NSString stringWithFormat:@"initWithIOService failed: %@", open_error]);
                return -1;
            }

            WfbMacosUsbHost *host = (WfbMacosUsbHost *)calloc(1, sizeof(WfbMacosUsbHost));
            if (!host) {
                [device destroy];
                set_error(error, error_len, @"calloc failed");
                return -1;
            }
            host->device = (__bridge_retained void *)device;
            *out_host = host;
            return 0;
        }

        IOObjectRelease(iterator);
        set_error(error, error_len, @"no matching IOUSBHostDevice");
        return -1;
    }
}

void wfb_macos_usbhost_close(WfbMacosUsbHost *host) {
    @autoreleasepool {
        if (!host) {
            return;
        }
        if (host->device) {
            IOUSBHostDevice *device = (__bridge_transfer IOUSBHostDevice *)host->device;
            [device destroy];
            host->device = NULL;
        }
        free(host);
    }
}

int wfb_macos_usbhost_control_read(WfbMacosUsbHost *host,
                                   uint8_t request_type,
                                   uint8_t request,
                                   uint16_t value,
                                   uint16_t index,
                                   uint8_t *data,
                                   size_t len,
                                   uint64_t timeout_ms,
                                   size_t *transferred,
                                   char *error,
                                   size_t error_len) {
    @autoreleasepool {
        if (!host || !host->device || !data || len > UINT16_MAX) {
            set_error(error, error_len, @"invalid control read argument");
            return -1;
        }
        IOUSBHostDevice *device = (__bridge IOUSBHostDevice *)host->device;
        IOUSBDeviceRequest req = {
            .bmRequestType = request_type,
            .bRequest = request,
            .wValue = value,
            .wIndex = index,
            .wLength = (uint16_t)len,
        };
        NSMutableData *buffer = [NSMutableData dataWithLength:len];
        NSUInteger actual = 0;
        NSError *request_error = nil;
        BOOL ok = [device sendDeviceRequest:req
                                       data:buffer
                           bytesTransferred:&actual
                          completionTimeout:(NSTimeInterval)timeout_ms / 1000.0
                                      error:&request_error];
        if (!ok) {
            set_error(error, error_len, [NSString stringWithFormat:@"control read failed: %@", request_error]);
            return -1;
        }
        memcpy(data, buffer.bytes, actual);
        if (transferred) {
            *transferred = actual;
        }
        return 0;
    }
}

int wfb_macos_usbhost_control_write(WfbMacosUsbHost *host,
                                    uint8_t request_type,
                                    uint8_t request,
                                    uint16_t value,
                                    uint16_t index,
                                    const uint8_t *data,
                                    size_t len,
                                    uint64_t timeout_ms,
                                    size_t *transferred,
                                    char *error,
                                    size_t error_len) {
    @autoreleasepool {
        if (!host || !host->device || (!data && len != 0) || len > UINT16_MAX) {
            set_error(error, error_len, @"invalid control write argument");
            return -1;
        }
        IOUSBHostDevice *device = (__bridge IOUSBHostDevice *)host->device;
        IOUSBDeviceRequest req = {
            .bmRequestType = request_type,
            .bRequest = request,
            .wValue = value,
            .wIndex = index,
            .wLength = (uint16_t)len,
        };
        NSMutableData *buffer = len == 0
            ? nil
            : [NSMutableData dataWithBytes:data length:len];
        NSUInteger actual = 0;
        NSError *request_error = nil;
        BOOL ok = [device sendDeviceRequest:req
                                       data:buffer
                           bytesTransferred:&actual
                          completionTimeout:(NSTimeInterval)timeout_ms / 1000.0
                                      error:&request_error];
        if (!ok) {
            set_error(error, error_len, [NSString stringWithFormat:@"control write failed: %@", request_error]);
            return -1;
        }
        if (transferred) {
            *transferred = actual;
        }
        return 0;
    }
}
