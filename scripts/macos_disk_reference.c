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
    if (IORegistryEntryGetRegistryEntryID(driver, &id) != KERN_SUCCESS) goto done;
    if (IORegistryEntryGetParentEntry(driver, kIOServicePlane, &parent) != KERN_SUCCESS) goto done;
    protocol = IORegistryEntryCreateCFProperty(parent, CFSTR(kIOPropertyProtocolCharacteristicsKey), NULL, 0);
    if (!protocol || CFGetTypeID(protocol) != CFDictionaryGetTypeID()) goto done;
    CFTypeRef interconnect = CFDictionaryGetValue((CFDictionaryRef)protocol, CFSTR(kIOPropertyPhysicalInterconnectTypeKey));
    if (!interconnect || CFGetTypeID(interconnect) != CFStringGetTypeID()) goto done;
    if (CFEqual(interconnect, CFSTR(kIOPropertyPhysicalInterconnectTypeVirtual))) {
        success = 1;
        goto done;
    }
    statistics = IORegistryEntryCreateCFProperty(driver, CFSTR(kIOBlockStorageDriverStatisticsKey), NULL, 0);
    if (!statistics || CFGetTypeID(statistics) != CFDictionaryGetTypeID()) goto done;
    if (!counter((CFDictionaryRef)statistics, CFSTR(kIOBlockStorageDriverStatisticsBytesReadKey), &read)) goto done;
    if (!counter((CFDictionaryRef)statistics, CFSTR(kIOBlockStorageDriverStatisticsBytesWrittenKey), &write)) goto done;
    printf("%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n", id, read, write);
    success = 1;
done:
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
    if (!matching) return 1;
    if (matching_services(0, matching, &iterator) != KERN_SUCCESS) return 1;
    if (!iterator) return 0;
    unsigned count = 0;
    int success = 1;
    io_object_t driver;
    while ((driver = IOIteratorNext(iterator))) {
        success = ++count <= 1024 && read_driver(driver);
        IOObjectRelease(driver);
        if (!success) break;
    }
    if (!iterator_valid(iterator)) success = 0;
    IOObjectRelease(iterator);
    return success ? 0 : 1;
}
