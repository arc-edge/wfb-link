#import <Foundation/Foundation.h>
#import <IOKit/IOKitLib.h>
#import <IOUSBHost/IOUSBHost.h>

static uint16_t parse_u16_arg(const char *text, const char *name) {
    char *end = NULL;
    unsigned long value = strtoul(text, &end, 0);
    if (end == text || *end != '\0' || value > 0xffff) {
        fprintf(stderr, "invalid %s: %s\n", name, text);
        exit(2);
    }
    return (uint16_t)value;
}

static uint8_t parse_u8_arg(const char *text, const char *name) {
    char *end = NULL;
    unsigned long value = strtoul(text, &end, 0);
    if (end == text || *end != '\0' || value > 0xff) {
        fprintf(stderr, "invalid %s: %s\n", name, text);
        exit(2);
    }
    return (uint8_t)value;
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

static NSString *string_property(io_service_t service, CFStringRef key) {
    CFTypeRef value = IORegistryEntryCreateCFProperty(service, key, kCFAllocatorDefault, 0);
    if (!value) {
        return nil;
    }
    NSString *out = nil;
    if (CFGetTypeID(value) == CFStringGetTypeID()) {
        out = [(__bridge NSString *)value copy];
    }
    CFRelease(value);
    return out;
}

static void print_usage(const char *argv0) {
    fprintf(stderr, "usage: %s --vid 0x0bda --pid 0x8812 [--config 1] [--read-reg 0x0002 --len 1] [--reset]\n", argv0);
}

int main(int argc, const char *argv[]) {
    @autoreleasepool {
        uint16_t wanted_vid = 0;
        uint16_t wanted_pid = 0;
        uint8_t config_value = 1;
        BOOL configure_device = YES;
        BOOL reset_after_configure = NO;
        BOOL read_register = NO;
        uint16_t read_register_address = 0;
        uint8_t read_length = 1;

        for (int i = 1; i < argc; i++) {
            if (strcmp(argv[i], "--vid") == 0 && i + 1 < argc) {
                wanted_vid = parse_u16_arg(argv[++i], "vid");
            } else if (strcmp(argv[i], "--pid") == 0 && i + 1 < argc) {
                wanted_pid = parse_u16_arg(argv[++i], "pid");
            } else if (strcmp(argv[i], "--config") == 0 && i + 1 < argc) {
                config_value = parse_u8_arg(argv[++i], "config");
                configure_device = YES;
            } else if (strcmp(argv[i], "--no-configure") == 0) {
                configure_device = NO;
            } else if (strcmp(argv[i], "--read-reg") == 0 && i + 1 < argc) {
                read_register_address = parse_u16_arg(argv[++i], "read-reg");
                read_register = YES;
            } else if (strcmp(argv[i], "--len") == 0 && i + 1 < argc) {
                read_length = parse_u8_arg(argv[++i], "len");
                if (read_length == 0) {
                    fprintf(stderr, "--len must be nonzero\n");
                    return 2;
                }
            } else if (strcmp(argv[i], "--reset") == 0) {
                reset_after_configure = YES;
            } else {
                print_usage(argv[0]);
                return 2;
            }
        }

        if (wanted_vid == 0 || wanted_pid == 0) {
            print_usage(argv[0]);
            return 2;
        }

        io_iterator_t iterator = IO_OBJECT_NULL;
        kern_return_t kr = IORegistryCreateIterator(kIOMainPortDefault,
                                                    kIOUSBPlane,
                                                    kIORegistryIterateRecursively,
                                                    &iterator);
        if (kr != KERN_SUCCESS) {
            fprintf(stderr, "IORegistryCreateIterator failed: 0x%08x\n", kr);
            return 1;
        }

        int matched = 0;
        int configured = 0;
        int failed_operations = 0;
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

            matched++;
            NSString *vendor = string_property(service, CFSTR("USB Vendor Name"));
            NSString *product = string_property(service, CFSTR("USB Product Name"));
            NSLog(@"matched %04x:%04x %@ %@", vid, pid, vendor ?: @"", product ?: @"");

            NSError *error = nil;
            IOUSBHostDevice *device = [[IOUSBHostDevice alloc] initWithIOService:service
                                                                            queue:nil
                                                                            error:&error
                                                                  interestHandler:nil];
            if (!device) {
                NSLog(@"initWithIOService failed: %@", error);
                IOObjectRelease(service);
                continue;
            }

            BOOL ok = YES;
            if (configure_device) {
                ok = [device configureWithValue:config_value matchInterfaces:YES error:&error];
                if (!ok) {
                    NSLog(@"configureWithValue:%u failed: %@", config_value, error);
                    failed_operations++;
                } else {
                    NSLog(@"configured %04x:%04x with configuration %u", vid, pid, config_value);
                    configured++;
                }
            } else {
                configured++;
            }

            if (ok && read_register) {
                IOUSBDeviceRequest request = {
                    .bmRequestType = 0xc0,
                    .bRequest = 0x05,
                    .wValue = read_register_address,
                    .wIndex = 0,
                    .wLength = read_length,
                };
                NSMutableData *data = [NSMutableData dataWithLength:read_length];
                NSUInteger transferred = 0;
                NSError *request_error = nil;
                BOOL request_ok = [device sendDeviceRequest:request
                                                       data:data
                                           bytesTransferred:&transferred
                                          completionTimeout:0.5
                                                      error:&request_error];
                if (!request_ok) {
                    NSLog(@"vendor read addr=0x%04x len=%u failed: %@", read_register_address, read_length, request_error);
                    ok = NO;
                    failed_operations++;
                } else {
                    const uint8_t *bytes = (const uint8_t *)data.bytes;
                    printf("read addr=0x%04x len=%lu data=", read_register_address, (unsigned long)transferred);
                    for (NSUInteger i = 0; i < transferred; i++) {
                        printf("%02x", bytes[i]);
                    }
                    printf("\n");
                }
            }

            if (ok && reset_after_configure) {
                NSError *reset_error = nil;
                BOOL reset_ok = [device resetWithError:&reset_error];
                if (!reset_ok) {
                    NSLog(@"resetWithError failed: %@", reset_error);
                    failed_operations++;
                } else {
                    NSLog(@"reset requested for %04x:%04x", vid, pid);
                }
            }

            [device destroy];
            IOObjectRelease(service);
        }

        IOObjectRelease(iterator);

        if (matched == 0) {
            fprintf(stderr, "no matching IOUSBHostDevice for %04x:%04x\n", wanted_vid, wanted_pid);
            return 1;
        }
        if (configured == 0) {
            fprintf(stderr, "matched %d device(s), configured none\n", matched);
            return 1;
        }
        if (failed_operations != 0) {
            fprintf(stderr, "matched %d device(s), %d operation(s) failed\n", matched, failed_operations);
            return 1;
        }

        return 0;
    }
}
