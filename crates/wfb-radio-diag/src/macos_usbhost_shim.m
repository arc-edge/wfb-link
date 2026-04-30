#import <Foundation/Foundation.h>
#import <IOKit/IOKitLib.h>
#import <IOUSBHost/IOUSBHost.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define WFB_MACOS_USBHOST_MAX_PIPE_PROBES 16
#define WFB_MACOS_USBHOST_PIPE_ERROR_LEN 160

typedef struct WfbMacosUsbHost {
    void *device;
} WfbMacosUsbHost;

typedef struct WfbMacosUsbHostPipeProbe {
    uint8_t address;
    int requested;
    int copied;
    int descriptor_available;
    uint8_t descriptor_address;
    uint8_t attributes;
    uint16_t max_packet_size;
    uint8_t interval;
    char error[WFB_MACOS_USBHOST_PIPE_ERROR_LEN];
} WfbMacosUsbHostPipeProbe;

typedef struct WfbMacosUsbHostInterfaceProbe {
    int configure_attempted;
    int configure_ok;
    int match_interfaces;
    int interface_found;
    int interface_opened;
    uint32_t poll_attempts_observed;
    uint32_t matched_interface_count;
    size_t pipe_count;
    WfbMacosUsbHostPipeProbe pipes[WFB_MACOS_USBHOST_MAX_PIPE_PROBES];
} WfbMacosUsbHostInterfaceProbe;

typedef struct WfbMacosUsbHostBulkTransfer {
    int configure_attempted;
    int configure_ok;
    int interface_found;
    int interface_opened;
    int pipe_copied;
    int descriptor_available;
    int transfer_ok;
    int timed_out;
    uint32_t poll_attempts_observed;
    uint32_t matched_interface_count;
    uint8_t endpoint_address;
    uint8_t descriptor_address;
    uint8_t attributes;
    uint16_t max_packet_size;
    uint8_t interval;
    size_t requested_len;
    size_t transferred_len;
} WfbMacosUsbHostBulkTransfer;

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

static void clear_error(char *error, size_t error_len) {
    if (error && error_len > 0) {
        error[0] = '\0';
    }
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

static io_service_t copy_first_matching_interface(uint16_t vid,
                                                  uint16_t pid,
                                                  uint8_t configuration_value,
                                                  uint8_t interface_number,
                                                  uint32_t *matched_count,
                                                  kern_return_t *last_kr) {
    if (matched_count) {
        *matched_count = 0;
    }
    if (last_kr) {
        *last_kr = KERN_SUCCESS;
    }

    CFMutableDictionaryRef matching = [IOUSBHostInterface createMatchingDictionaryWithVendorID:@(vid)
                                                                                     productID:@(pid)
                                                                                     bcdDevice:nil
                                                                               interfaceNumber:@(interface_number)
                                                                            configurationValue:@(configuration_value)
                                                                                interfaceClass:nil
                                                                             interfaceSubclass:nil
                                                                             interfaceProtocol:nil
                                                                                         speed:nil
                                                                                productIDArray:nil];
    if (!matching) {
        if (last_kr) {
            *last_kr = kIOReturnNoMemory;
        }
        return IO_OBJECT_NULL;
    }

    io_iterator_t iterator = IO_OBJECT_NULL;
    kern_return_t kr = IOServiceGetMatchingServices(kIOMainPortDefault, matching, &iterator);
    if (last_kr) {
        *last_kr = kr;
    }
    if (kr != KERN_SUCCESS) {
        return IO_OBJECT_NULL;
    }

    io_service_t first = IO_OBJECT_NULL;
    io_service_t service = IO_OBJECT_NULL;
    while ((service = IOIteratorNext(iterator)) != IO_OBJECT_NULL) {
        if (matched_count) {
            *matched_count += 1;
        }
        if (first == IO_OBJECT_NULL) {
            first = service;
        } else {
            IOObjectRelease(service);
        }
    }
    IOObjectRelease(iterator);
    return first;
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

int wfb_macos_usbhost_interface_probe(WfbMacosUsbHost *host,
                                      uint16_t vid,
                                      uint16_t pid,
                                      uint8_t configuration_value,
                                      int match_interfaces,
                                      uint8_t interface_number,
                                      const uint8_t *pipe_addresses,
                                      size_t pipe_count,
                                      uint32_t poll_attempts,
                                      uint64_t poll_delay_ms,
                                      WfbMacosUsbHostInterfaceProbe *result,
                                      char *error,
                                      size_t error_len) {
    @autoreleasepool {
        clear_error(error, error_len);
        if (!host || !host->device || !result || (!pipe_addresses && pipe_count != 0)) {
            set_error(error, error_len, @"invalid interface probe argument");
            return -1;
        }
        if (pipe_count > WFB_MACOS_USBHOST_MAX_PIPE_PROBES) {
            set_error(error, error_len, [NSString stringWithFormat:@"pipe_count %zu exceeds max %d", pipe_count, WFB_MACOS_USBHOST_MAX_PIPE_PROBES]);
            return -1;
        }

        memset(result, 0, sizeof(*result));
        result->configure_attempted = 1;
        result->match_interfaces = match_interfaces ? 1 : 0;
        result->pipe_count = pipe_count;
        for (size_t i = 0; i < pipe_count; i++) {
            result->pipes[i].address = pipe_addresses[i];
            result->pipes[i].requested = 1;
            clear_error(result->pipes[i].error, sizeof(result->pipes[i].error));
        }

        IOUSBHostDevice *device = (__bridge IOUSBHostDevice *)host->device;
        NSError *configure_error = nil;
        BOOL configured = [device configureWithValue:configuration_value
                                     matchInterfaces:(match_interfaces ? YES : NO)
                                               error:&configure_error];
        result->configure_ok = configured ? 1 : 0;
        if (!configured) {
            set_error(error, error_len, [NSString stringWithFormat:@"configureWithValue:%u matchInterfaces:%@ failed: %@", configuration_value, match_interfaces ? @"YES" : @"NO", configure_error]);
            return 0;
        }

        io_service_t interface_service = IO_OBJECT_NULL;
        kern_return_t last_kr = KERN_SUCCESS;
        uint32_t last_matched_count = 0;
        uint32_t attempts = poll_attempts == 0 ? 1 : poll_attempts;
        for (uint32_t attempt = 1; attempt <= attempts; attempt++) {
            interface_service = copy_first_matching_interface(vid,
                                                              pid,
                                                              configuration_value,
                                                              interface_number,
                                                              &last_matched_count,
                                                              &last_kr);
            result->poll_attempts_observed = attempt;
            result->matched_interface_count = last_matched_count;
            if (interface_service != IO_OBJECT_NULL) {
                result->interface_found = 1;
                break;
            }
            if (attempt < attempts && poll_delay_ms > 0) {
                usleep((useconds_t)(poll_delay_ms * 1000));
            }
        }

        if (interface_service == IO_OBJECT_NULL) {
            set_error(error, error_len, [NSString stringWithFormat:@"no IOUSBHostInterface matched vid=0x%04x pid=0x%04x configuration=%u interface=%u after %u poll(s); last IOKit status=0x%08x",
                                         vid,
                                         pid,
                                         configuration_value,
                                         interface_number,
                                         result->poll_attempts_observed,
                                         last_kr]);
            return 0;
        }

        NSError *interface_error = nil;
        IOUSBHostInterface *interface = [[IOUSBHostInterface alloc] initWithIOService:interface_service
                                                                              options:0
                                                                                queue:nil
                                                                                error:&interface_error
                                                                      interestHandler:nil];
        IOObjectRelease(interface_service);
        if (!interface) {
            set_error(error, error_len, [NSString stringWithFormat:@"IOUSBHostInterface initWithIOService failed: %@", interface_error]);
            return 0;
        }

        result->interface_opened = 1;
        for (size_t i = 0; i < pipe_count; i++) {
            NSError *pipe_error = nil;
            IOUSBHostPipe *pipe = [interface copyPipeWithAddress:pipe_addresses[i]
                                                           error:&pipe_error];
            if (!pipe) {
                set_error(result->pipes[i].error, sizeof(result->pipes[i].error), [NSString stringWithFormat:@"copyPipeWithAddress:0x%02x failed: %@", pipe_addresses[i], pipe_error]);
                continue;
            }

            result->pipes[i].copied = 1;
            result->pipes[i].descriptor_address = (uint8_t)pipe.endpointAddress;
            const IOUSBHostIOSourceDescriptors *descriptors = pipe.descriptors;
            if (descriptors) {
                result->pipes[i].descriptor_available = 1;
                result->pipes[i].descriptor_address = descriptors->descriptor.bEndpointAddress;
                result->pipes[i].attributes = descriptors->descriptor.bmAttributes;
                result->pipes[i].max_packet_size = descriptors->descriptor.wMaxPacketSize;
                result->pipes[i].interval = descriptors->descriptor.bInterval;
            }
        }

        [interface destroy];
        return 0;
    }
}

int wfb_macos_usbhost_bulk_read_once(WfbMacosUsbHost *host,
                                     uint16_t vid,
                                     uint16_t pid,
                                     uint8_t configuration_value,
                                     int match_interfaces,
                                     uint8_t interface_number,
                                     uint8_t endpoint_address,
                                     uint8_t *data,
                                     size_t len,
                                     uint32_t poll_attempts,
                                     uint64_t poll_delay_ms,
                                     uint64_t timeout_ms,
                                     WfbMacosUsbHostBulkTransfer *result,
                                     char *error,
                                     size_t error_len) {
    @autoreleasepool {
        clear_error(error, error_len);
        if (!host || !host->device || !result || (!data && len != 0)) {
            set_error(error, error_len, @"invalid bulk read argument");
            return -1;
        }

        memset(result, 0, sizeof(*result));
        result->configure_attempted = 1;
        result->endpoint_address = endpoint_address;
        result->requested_len = len;

        IOUSBHostDevice *device = (__bridge IOUSBHostDevice *)host->device;
        NSError *configure_error = nil;
        BOOL configured = [device configureWithValue:configuration_value
                                     matchInterfaces:(match_interfaces ? YES : NO)
                                               error:&configure_error];
        result->configure_ok = configured ? 1 : 0;
        if (!configured) {
            set_error(error, error_len, [NSString stringWithFormat:@"configureWithValue:%u matchInterfaces:%@ failed: %@", configuration_value, match_interfaces ? @"YES" : @"NO", configure_error]);
            return 0;
        }

        io_service_t interface_service = IO_OBJECT_NULL;
        kern_return_t last_kr = KERN_SUCCESS;
        uint32_t last_matched_count = 0;
        uint32_t attempts = poll_attempts == 0 ? 1 : poll_attempts;
        for (uint32_t attempt = 1; attempt <= attempts; attempt++) {
            interface_service = copy_first_matching_interface(vid,
                                                              pid,
                                                              configuration_value,
                                                              interface_number,
                                                              &last_matched_count,
                                                              &last_kr);
            result->poll_attempts_observed = attempt;
            result->matched_interface_count = last_matched_count;
            if (interface_service != IO_OBJECT_NULL) {
                result->interface_found = 1;
                break;
            }
            if (attempt < attempts && poll_delay_ms > 0) {
                usleep((useconds_t)(poll_delay_ms * 1000));
            }
        }

        if (interface_service == IO_OBJECT_NULL) {
            set_error(error, error_len, [NSString stringWithFormat:@"no IOUSBHostInterface matched vid=0x%04x pid=0x%04x configuration=%u interface=%u after %u poll(s); last IOKit status=0x%08x",
                                         vid,
                                         pid,
                                         configuration_value,
                                         interface_number,
                                         result->poll_attempts_observed,
                                         last_kr]);
            return 0;
        }

        NSError *interface_error = nil;
        IOUSBHostInterface *interface = [[IOUSBHostInterface alloc] initWithIOService:interface_service
                                                                              options:0
                                                                                queue:nil
                                                                                error:&interface_error
                                                                      interestHandler:nil];
        IOObjectRelease(interface_service);
        if (!interface) {
            set_error(error, error_len, [NSString stringWithFormat:@"IOUSBHostInterface initWithIOService failed: %@", interface_error]);
            return 0;
        }
        result->interface_opened = 1;

        NSError *pipe_error = nil;
        IOUSBHostPipe *pipe = [interface copyPipeWithAddress:endpoint_address
                                                       error:&pipe_error];
        if (!pipe) {
            set_error(error, error_len, [NSString stringWithFormat:@"copyPipeWithAddress:0x%02x failed: %@", endpoint_address, pipe_error]);
            [interface destroy];
            return 0;
        }
        result->pipe_copied = 1;
        result->descriptor_address = (uint8_t)pipe.endpointAddress;
        const IOUSBHostIOSourceDescriptors *descriptors = pipe.descriptors;
        if (descriptors) {
            result->descriptor_available = 1;
            result->descriptor_address = descriptors->descriptor.bEndpointAddress;
            result->attributes = descriptors->descriptor.bmAttributes;
            result->max_packet_size = descriptors->descriptor.wMaxPacketSize;
            result->interval = descriptors->descriptor.bInterval;
        }

        NSMutableData *buffer = [NSMutableData dataWithLength:len];
        NSUInteger actual = 0;
        NSError *request_error = nil;
        BOOL ok = [pipe sendIORequestWithData:buffer
                             bytesTransferred:&actual
                            completionTimeout:(NSTimeInterval)timeout_ms / 1000.0
                                        error:&request_error];
        result->transfer_ok = ok ? 1 : 0;
        result->transferred_len = actual;
        if (ok) {
            memcpy(data, buffer.bytes, actual);
        } else {
            if (request_error && request_error.code == kIOReturnTimeout) {
                result->timed_out = 1;
            }
            set_error(error, error_len, [NSString stringWithFormat:@"bulk read endpoint 0x%02x failed after %llu ms: %@", endpoint_address, (unsigned long long)timeout_ms, request_error]);
        }

        [interface destroy];
        return 0;
    }
}

int wfb_macos_usbhost_bulk_write_once(WfbMacosUsbHost *host,
                                      uint16_t vid,
                                      uint16_t pid,
                                      uint8_t configuration_value,
                                      int match_interfaces,
                                      uint8_t interface_number,
                                      uint8_t endpoint_address,
                                      const uint8_t *data,
                                      size_t len,
                                      uint32_t poll_attempts,
                                      uint64_t poll_delay_ms,
                                      uint64_t timeout_ms,
                                      WfbMacosUsbHostBulkTransfer *result,
                                      char *error,
                                      size_t error_len) {
    @autoreleasepool {
        clear_error(error, error_len);
        if (!host || !host->device || !result || (!data && len != 0)) {
            set_error(error, error_len, @"invalid bulk write argument");
            return -1;
        }

        memset(result, 0, sizeof(*result));
        result->configure_attempted = 1;
        result->endpoint_address = endpoint_address;
        result->requested_len = len;

        IOUSBHostDevice *device = (__bridge IOUSBHostDevice *)host->device;
        NSError *configure_error = nil;
        BOOL configured = [device configureWithValue:configuration_value
                                     matchInterfaces:(match_interfaces ? YES : NO)
                                               error:&configure_error];
        result->configure_ok = configured ? 1 : 0;
        if (!configured) {
            set_error(error, error_len, [NSString stringWithFormat:@"configureWithValue:%u matchInterfaces:%@ failed: %@", configuration_value, match_interfaces ? @"YES" : @"NO", configure_error]);
            return 0;
        }

        io_service_t interface_service = IO_OBJECT_NULL;
        kern_return_t last_kr = KERN_SUCCESS;
        uint32_t last_matched_count = 0;
        uint32_t attempts = poll_attempts == 0 ? 1 : poll_attempts;
        for (uint32_t attempt = 1; attempt <= attempts; attempt++) {
            interface_service = copy_first_matching_interface(vid,
                                                              pid,
                                                              configuration_value,
                                                              interface_number,
                                                              &last_matched_count,
                                                              &last_kr);
            result->poll_attempts_observed = attempt;
            result->matched_interface_count = last_matched_count;
            if (interface_service != IO_OBJECT_NULL) {
                result->interface_found = 1;
                break;
            }
            if (attempt < attempts && poll_delay_ms > 0) {
                usleep((useconds_t)(poll_delay_ms * 1000));
            }
        }

        if (interface_service == IO_OBJECT_NULL) {
            set_error(error, error_len, [NSString stringWithFormat:@"no IOUSBHostInterface matched vid=0x%04x pid=0x%04x configuration=%u interface=%u after %u poll(s); last IOKit status=0x%08x",
                                         vid,
                                         pid,
                                         configuration_value,
                                         interface_number,
                                         result->poll_attempts_observed,
                                         last_kr]);
            return 0;
        }

        NSError *interface_error = nil;
        IOUSBHostInterface *interface = [[IOUSBHostInterface alloc] initWithIOService:interface_service
                                                                              options:0
                                                                                queue:nil
                                                                                error:&interface_error
                                                                      interestHandler:nil];
        IOObjectRelease(interface_service);
        if (!interface) {
            set_error(error, error_len, [NSString stringWithFormat:@"IOUSBHostInterface initWithIOService failed: %@", interface_error]);
            return 0;
        }
        result->interface_opened = 1;

        NSError *pipe_error = nil;
        IOUSBHostPipe *pipe = [interface copyPipeWithAddress:endpoint_address
                                                       error:&pipe_error];
        if (!pipe) {
            set_error(error, error_len, [NSString stringWithFormat:@"copyPipeWithAddress:0x%02x failed: %@", endpoint_address, pipe_error]);
            [interface destroy];
            return 0;
        }
        result->pipe_copied = 1;
        result->descriptor_address = (uint8_t)pipe.endpointAddress;
        const IOUSBHostIOSourceDescriptors *descriptors = pipe.descriptors;
        if (descriptors) {
            result->descriptor_available = 1;
            result->descriptor_address = descriptors->descriptor.bEndpointAddress;
            result->attributes = descriptors->descriptor.bmAttributes;
            result->max_packet_size = descriptors->descriptor.wMaxPacketSize;
            result->interval = descriptors->descriptor.bInterval;
        }

        NSMutableData *buffer = len == 0
            ? nil
            : [NSMutableData dataWithBytes:data length:len];
        NSUInteger actual = 0;
        NSError *request_error = nil;
        BOOL ok = [pipe sendIORequestWithData:buffer
                             bytesTransferred:&actual
                            completionTimeout:(NSTimeInterval)timeout_ms / 1000.0
                                        error:&request_error];
        result->transfer_ok = ok ? 1 : 0;
        result->transferred_len = actual;
        if (!ok) {
            if (request_error && request_error.code == kIOReturnTimeout) {
                result->timed_out = 1;
            }
            set_error(error, error_len, [NSString stringWithFormat:@"bulk write endpoint 0x%02x failed after %llu ms: %@", endpoint_address, (unsigned long long)timeout_ms, request_error]);
        }

        [interface destroy];
        return 0;
    }
}
