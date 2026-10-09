/* Independent SDK-header reference for temporal bounds and FFI ABI verification. */
#include <CoreFoundation/CoreFoundation.h>
#include <IOKit/IOKitLib.h>
#include <IOKit/storage/IOBlockStorageDriver.h>
#include <IOKit/storage/IOStorageProtocolCharacteristics.h>
#include <inttypes.h>
#include <stdio.h>

_Static_assert(sizeof(io_object_t) == 4, "IOKit handle ABI");
_Static_assert(sizeof(kern_return_t) == 4, "kernel status ABI");
_Static_assert(sizeof(boolean_t) == 4, "Mach Boolean ABI");
_Static_assert(sizeof(Boolean) == 1, "CF Boolean ABI");
_Static_assert(sizeof(CFIndex) == 8, "64-bit macOS only");
_Static_assert(sizeof(CFNumberType) == sizeof(CFIndex), "CFNumberType is CFIndex");
#if defined(__arm64__)
_Static_assert((boolean_t)-1 < 0, "arm64 Mach Boolean signedness");
#else
_Static_assert((boolean_t)-1 > 0, "x86_64 Mach Boolean signedness");
#endif

static int counter(CFDictionaryRef table, CFStringRef key, uint64_t *out) {
    CFTypeRef value = CFDictionaryGetValue(table, key);
    int64_t signed_value = 0;
    if (!value || CFGetTypeID(value) != CFNumberGetTypeID() || CFNumberIsFloatType(value)) return 0;
    if (!CFNumberGetValue(value, kCFNumberSInt64Type, &signed_value) || signed_value < 0) return 0;
    *out = (uint64_t)signed_value;
    return 1;
}

static int read_driver(io_object_t driver) {
    uint64_t id = 0, read = 0, write = 0;
    io_registry_entry_t parent = IO_OBJECT_NULL;
    CFTypeRef protocol = NULL, statistics = NULL;
    int success = 0;
    const char *stage = "registry identity";
    if (IORegistryEntryGetRegistryEntryID(driver, &id) != KERN_SUCCESS) goto done;
    stage = "provider parent";
    if (IORegistryEntryGetParentEntry(driver, kIOServicePlane, &parent) != KERN_SUCCESS) goto done;
    stage = "provider Protocol Characteristics dictionary";
    protocol = IORegistryEntryCreateCFProperty(parent, CFSTR(kIOPropertyProtocolCharacteristicsKey), NULL, 0);
    if (!protocol || CFGetTypeID(protocol) != CFDictionaryGetTypeID()) goto done;
    stage = "provider Physical Interconnect string";
    CFTypeRef interconnect = CFDictionaryGetValue((CFDictionaryRef)protocol, CFSTR(kIOPropertyPhysicalInterconnectTypeKey));
    if (!interconnect || CFGetTypeID(interconnect) != CFStringGetTypeID()) goto done;
    if (CFEqual(interconnect, CFSTR(kIOPropertyPhysicalInterconnectTypeVirtual))) {
        success = 1;
        goto done;
    }
    stage = "driver Statistics dictionary";
    statistics = IORegistryEntryCreateCFProperty(driver, CFSTR(kIOBlockStorageDriverStatisticsKey), NULL, 0);
    if (!statistics || CFGetTypeID(statistics) != CFDictionaryGetTypeID()) goto done;
    stage = "driver Bytes (Read) integer";
    if (!counter((CFDictionaryRef)statistics, CFSTR(kIOBlockStorageDriverStatisticsBytesReadKey), &read)) goto done;
    stage = "driver Bytes (Write) integer";
    if (!counter((CFDictionaryRef)statistics, CFSTR(kIOBlockStorageDriverStatisticsBytesWrittenKey), &write)) goto done;
    printf("%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n", id, read, write);
    success = 1;
done:
    if (!success) {
        io_name_t class_name = {0};
        (void)IOObjectGetClass(driver, class_name);
        fprintf(stderr, "driver %" PRIu64 " (%s): failed at %s\n", id, class_name, stage);
        if (parent) {
            io_name_t parent_class = {0};
            (void)IOObjectGetClass(parent, parent_class);
            fprintf(stderr, "provider class: %s\n", parent_class);
        }
        if (protocol) {
            fputs("provider protocol metadata:\n", stderr);
            CFShow(protocol);
        }
    }
    if (statistics) CFRelease(statistics);
    if (protocol) CFRelease(protocol);
    if (parent) IOObjectRelease(parent);
    return success;
}

int main(void) {
    /* Assignments verify the function signatures mirrored by Rust, using the SDK. */
    Boolean (*number_value)(CFNumberRef, CFIndex, void *) = CFNumberGetValue;
    boolean_t (*iterator_valid)(io_iterator_t) = IOIteratorIsValid;
    kern_return_t (*matching_services)(mach_port_t, CFDictionaryRef, io_iterator_t *) = IOServiceGetMatchingServices;
    (void)number_value;
    io_iterator_t iterator = IO_OBJECT_NULL;
    CFMutableDictionaryRef matching = IOServiceMatching(kIOBlockStorageDriverClass);
    if (!matching) { fputs("IOServiceMatching returned null\n", stderr); return 1; }
    kern_return_t status = matching_services(0, matching, &iterator);
    if (status != KERN_SUCCESS) {
        fprintf(stderr, "IOServiceGetMatchingServices: 0x%x\n", status);
        return 1;
    }
    if (!iterator) return 0;
    unsigned count = 0;
    int success = 1;
    io_object_t driver;
    while ((driver = IOIteratorNext(iterator))) {
        success = ++count <= 1024 && read_driver(driver);
        if (count > 1024) fputs("driver scan limit exceeded\n", stderr);
        IOObjectRelease(driver);
        if (!success) break;
    }
    if (!iterator_valid(iterator)) {
        fputs("IOIteratorIsValid returned false\n", stderr);
        success = 0;
    }
    IOObjectRelease(iterator);
    return success ? 0 : 1;
}
