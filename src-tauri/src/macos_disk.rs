//! Cumulative byte counters per `IOBlockStorageDriver` registry entry (not per volume).
//!
//! `sample` re-enumerates on every call and keeps no native handles or identity cache.
//! A successful snapshot therefore retires missing IDs. The parent must replace its
//! active ID set, baseline new/reappearing IDs, and handle counter resets; never sum
//! these lifetime counters as session totals. IDs are valid only until reboot.
//!
//! Apple sources used to verify property names, ownership and ABI:
//! <https://github.com/apple-oss-distributions/IOKitUser/blob/main/IOKitLib.h>
//! <https://github.com/apple-oss-distributions/IOStorageFamily/blob/main/IOBlockStorageDriver.h>
//! <https://github.com/apple-oss-distributions/IOStorageFamily/blob/main/IOStorageProtocolCharacteristics.h>
//! <https://github.com/apple-oss-distributions/CF/blob/main/CFNumber.h>
//! <https://github.com/apple-oss-distributions/xnu/blob/main/iokit/IOKit/IOTypes.h>

#[cfg(any(target_os = "macos", test))]
const MAX_DRIVERS: usize = 1024;

#[cfg(any(target_os = "macos", test))]
fn snapshot(
    entries: impl IntoIterator<Item = Result<Option<(u64, u64, u64)>, String>>,
) -> Result<Vec<(u64, u64, u64)>, String> {
    let mut drivers = std::collections::BTreeMap::new();
    // Count scanned entries, including duplicates and virtual devices, not just output.
    for (index, entry) in entries.into_iter().enumerate() {
        if index >= MAX_DRIVERS {
            return Err(format!(
                "IOBlockStorageDriver scan exceeded {MAX_DRIVERS} entries"
            ));
        }
        if let Some((id, read, write)) = entry? {
            drivers.entry(id).or_insert((read, write));
        }
    }
    Ok(drivers
        .into_iter()
        .map(|(id, (read, write))| (id, read, write))
        .collect())
}

#[cfg(any(target_os = "macos", test))]
fn checked_counter(value: i64, exact: bool) -> Result<u64, String> {
    if !exact {
        return Err("disk counter cannot be converted exactly to signed 64-bit integer".into());
    }
    u64::try_from(value).map_err(|_| "negative or overflowing disk counter".into())
}

/// Returns `(registry_entry_id, cumulative_read_bytes, cumulative_write_bytes)`.
/// Virtual providers are excluded. Missing/malformed required properties, topology
/// invalidation, and over-limit scans return an error, never a partial snapshot.
/// Enumeration is bounded, but synchronous `IOKit` calls have no userspace timeout.
///
/// # Errors
/// Returns an error on unsupported platforms, malformed counters or failed `IOKit` calls.
#[cfg(target_os = "macos")]
pub fn sample() -> Result<Vec<(u64, u64, u64)>, String> {
    native::sample()
}

#[cfg(not(target_os = "macos"))]
/// # Errors
/// Native `IOKit` sampling is unavailable outside macOS.
pub fn sample() -> Result<Vec<(u64, u64, u64)>, String> {
    Err("native disk I/O sampling requires macOS".into())
}

#[cfg(target_os = "macos")]
mod native {
    use std::{
        ffi::{c_char, c_void, CStr},
        ptr,
    };

    type CfRef = *const c_void;
    // Mach boolean_t differs in signedness by architecture; CF Boolean is a byte.
    #[cfg(target_arch = "aarch64")]
    type MachBoolean = i32;
    #[cfg(not(target_arch = "aarch64"))]
    type MachBoolean = u32;

    #[link(name = "IOKit", kind = "framework")]
    extern "C" {
        fn IOServiceMatching(name: *const c_char) -> *mut c_void;
        fn IOServiceGetMatchingServices(port: u32, matching: CfRef, iterator: *mut u32) -> i32;
        fn IOIteratorNext(iterator: u32) -> u32;
        fn IOIteratorIsValid(iterator: u32) -> MachBoolean;
        fn IOObjectRelease(object: u32) -> i32;
        fn IORegistryEntryGetRegistryEntryID(entry: u32, id: *mut u64) -> i32;
        fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32)
            -> i32;
        fn IORegistryEntryCreateCFProperty(
            entry: u32,
            key: CfRef,
            allocator: CfRef,
            options: u32,
        ) -> CfRef;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFRelease(object: CfRef);
        fn CFGetTypeID(object: CfRef) -> usize;
        fn CFDictionaryGetTypeID() -> usize;
        fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
        fn CFStringGetTypeID() -> usize;
        fn CFStringCreateWithCString(
            allocator: CfRef,
            string: *const c_char,
            encoding: u32,
        ) -> CfRef;
        fn CFEqual(left: CfRef, right: CfRef) -> u8;
        fn CFNumberGetTypeID() -> usize;
        fn CFNumberIsFloatType(number: CfRef) -> u8;
        // CFNumberType is CFIndex (signed long), not a 32-bit C enum on LP64.
        fn CFNumberGetValue(number: CfRef, number_type: isize, value: *mut c_void) -> u8;
    }

    struct IoObject(u32);

    impl IoObject {
        fn new(raw: u32) -> Option<Self> {
            (raw != 0).then(|| Self(raw))
        }
    }

    impl Drop for IoObject {
        fn drop(&mut self) {
            // Each returned non-null IOKit handle owns one reference.
            unsafe {
                IOObjectRelease(self.0);
            }
        }
    }

    struct CfObject(CfRef);

    impl CfObject {
        fn new(raw: CfRef) -> Result<Self, String> {
            if raw.is_null() {
                Err("IOKit/CoreFoundation returned a null property or allocation".into())
            } else {
                Ok(Self(raw))
            }
        }

        fn string(value: &CStr) -> Result<Self, String> {
            // kCFStringEncodingUTF8; NULL allocator means the default allocator.
            Self::new(unsafe {
                CFStringCreateWithCString(ptr::null(), value.as_ptr(), 0x0800_0100)
            })
        }

        fn into_raw(self) -> CfRef {
            let raw = self.0;
            std::mem::forget(self);
            raw
        }
    }

    impl Drop for CfObject {
        fn drop(&mut self) {
            unsafe {
                CFRelease(self.0);
            }
        }
    }

    struct Keys {
        statistics: CfObject,
        read: CfObject,
        write: CfObject,
        protocol: CfObject,
        interconnect: CfObject,
        virtual_interface: CfObject,
    }

    impl Keys {
        fn new() -> Result<Self, String> {
            Ok(Self {
                statistics: CfObject::string(c"Statistics")?,
                read: CfObject::string(c"Bytes (Read)")?,
                write: CfObject::string(c"Bytes (Write)")?,
                protocol: CfObject::string(c"Protocol Characteristics")?,
                interconnect: CfObject::string(c"Physical Interconnect")?,
                virtual_interface: CfObject::string(c"Virtual Interface")?,
            })
        }
    }

    fn property(entry: u32, key: &CfObject) -> Result<CfObject, String> {
        CfObject::new(unsafe { IORegistryEntryCreateCFProperty(entry, key.0, ptr::null(), 0) })
    }

    fn dictionary_value(dictionary: &CfObject, key: &CfObject) -> Result<CfRef, String> {
        unsafe {
            if CFGetTypeID(dictionary.0) != CFDictionaryGetTypeID() {
                return Err("disk property is not a CFDictionary".into());
            }
            let value = CFDictionaryGetValue(dictionary.0, key.0);
            if value.is_null() {
                Err("required disk dictionary key is missing".into())
            } else {
                // Borrowed value: it is valid while dictionary lives; do not CFRelease.
                Ok(value)
            }
        }
    }

    fn counter(dictionary: &CfObject, key: &CfObject) -> Result<u64, String> {
        let number = dictionary_value(dictionary, key)?;
        unsafe {
            if CFGetTypeID(number) != CFNumberGetTypeID() || CFNumberIsFloatType(number) != 0 {
                return Err("disk counter is not an integer CFNumber".into());
            }
            let mut value = 0_i64;
            let exact = CFNumberGetValue(number, 4, ptr::from_mut(&mut value).cast()) != 0;
            super::checked_counter(value, exact)
        }
    }

    fn read_driver(driver: &IoObject, keys: &Keys) -> Result<Option<(u64, u64, u64)>, String> {
        let mut id = 0;
        let status = unsafe { IORegistryEntryGetRegistryEntryID(driver.0, ptr::from_mut(&mut id)) };
        if status != 0 {
            return Err(format!(
                "IORegistryEntryGetRegistryEntryID failed: {status}"
            ));
        }
        // IOBlockStorageDriver's direct provider holds Protocol Characteristics.
        // No recursive registry walk (which could be unbounded) is necessary.
        let mut raw_parent = 0;
        let status = unsafe {
            IORegistryEntryGetParentEntry(
                driver.0,
                c"IOService".as_ptr(),
                ptr::from_mut(&mut raw_parent),
            )
        };
        let parent = IoObject::new(raw_parent);
        if status != 0 {
            return Err(format!(
                "disk {id}: IORegistryEntryGetParentEntry failed: {status}"
            ));
        }
        let parent = parent.ok_or_else(|| format!("disk {id}: missing block storage provider"))?;
        let protocol = property(parent.0, &keys.protocol)?;
        let interconnect = dictionary_value(&protocol, &keys.interconnect)?;
        unsafe {
            if CFGetTypeID(interconnect) != CFStringGetTypeID() {
                return Err(format!(
                    "disk {id}: Physical Interconnect is not a CFString"
                ));
            }
            if CFEqual(interconnect, keys.virtual_interface.0) != 0 {
                return Ok(None);
            }
        }
        let statistics = property(driver.0, &keys.statistics)?;
        let read = counter(&statistics, &keys.read).map_err(|e| format!("disk {id} read: {e}"))?;
        let write =
            counter(&statistics, &keys.write).map_err(|e| format!("disk {id} write: {e}"))?;
        Ok(Some((id, read, write)))
    }

    pub(super) fn sample() -> Result<Vec<(u64, u64, u64)>, String> {
        let keys = Keys::new()?;
        let matching = CfObject::new(unsafe {
            IOServiceMatching(c"IOBlockStorageDriver".as_ptr()).cast_const()
        })?;
        let mut raw_iterator = 0;
        // This function consumes matching even on failure. Passing 0 uses the default
        // main port without referencing the macOS-12-only kIOMainPortDefault symbol.
        let status = unsafe {
            IOServiceGetMatchingServices(0, matching.into_raw(), ptr::from_mut(&mut raw_iterator))
        };
        let iterator = IoObject::new(raw_iterator);
        if status != 0 {
            return Err(format!("IOServiceGetMatchingServices failed: {status}"));
        }
        let Some(iterator) = iterator else {
            return Ok(Vec::new());
        };
        let records = std::iter::from_fn(|| {
            let driver = IoObject::new(unsafe { IOIteratorNext(iterator.0) })?;
            Some(read_driver(&driver, &keys))
        });
        let result = super::snapshot(records)?;
        if unsafe { IOIteratorIsValid(iterator.0) } == 0 {
            return Err("disk registry changed during enumeration; retry on the next tick".into());
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_counter, snapshot, MAX_DRIVERS};

    #[test]
    fn duplicate_driver_ids_are_counted_once_not_summed() {
        let result = snapshot([Ok(Some((9, 100, 200))), Ok(Some((9, 110, 220)))]).unwrap();
        assert_eq!(result, vec![(9, 100, 200)]);
    }

    #[test]
    fn distinct_drivers_are_sorted_and_keep_their_own_counters() {
        assert_eq!(
            snapshot([Ok(Some((8, 1, 2))), Ok(Some((3, 1, 2)))]).unwrap(),
            vec![(3, 1, 2), (8, 1, 2)]
        );
    }

    #[test]
    fn missing_ids_are_retired_and_reappearance_has_no_stale_counter() {
        assert_eq!(
            snapshot([Ok(Some((7, 900, 800)))]).unwrap(),
            vec![(7, 900, 800)]
        );
        assert_eq!(snapshot([]).unwrap(), Vec::<(u64, u64, u64)>::new());
        assert_eq!(snapshot([Ok(Some((7, 2, 3)))]).unwrap(), vec![(7, 2, 3)]);
    }

    #[test]
    fn excluded_virtual_drivers_do_not_become_zero_counter_records() {
        assert_eq!(
            snapshot([Ok(None), Ok(Some((2, 0, 0)))]).unwrap(),
            vec![(2, 0, 0)]
        );
    }

    #[test]
    fn exact_limit_is_allowed() {
        let entries = (0..MAX_DRIVERS).map(|id| Ok(Some((id as u64, 0, 0))));
        assert_eq!(snapshot(entries).unwrap().len(), MAX_DRIVERS);
    }

    #[test]
    fn over_limit_is_an_error_not_a_partial_snapshot() {
        assert!(snapshot((0..=MAX_DRIVERS).map(|id| Ok(Some((id as u64, 0, 0))))).is_err());
    }

    #[test]
    fn duplicates_and_exclusions_cannot_bypass_the_scan_bound() {
        assert!(snapshot(std::iter::repeat(Ok(Some((1, 0, 0))))).is_err());
        assert!(snapshot(std::iter::repeat(Ok(None))).is_err());
    }

    #[test]
    fn native_error_is_propagated_without_partial_success() {
        let error = snapshot([Ok(Some((1, 2, 3))), Err("unplugged".into())]).unwrap_err();
        assert_eq!(error, "unplugged");
    }

    #[test]
    fn counters_are_nonnegative_exact_signed_64_bit_integers() {
        assert_eq!(checked_counter(0, true).unwrap(), 0);
        assert_eq!(checked_counter(i64::MAX, true).unwrap(), i64::MAX as u64);
        assert!(checked_counter(-1, true).is_err());
        assert!(checked_counter(42, false).is_err());
    }
}
