//! Raw FFI for the IOKit calls we need. We avoid pulling in a heavy
//! IOKit crate because we only need a handful of symbols.

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]

use std::ffi::c_void;

pub type io_object_t = u32;
pub type io_service_t = io_object_t;
pub type IOReturn = i32;
pub type kern_return_t = i32;
pub type natural_t = u32;
pub type mach_port_t = u32;

pub const IO_OBJECT_NULL: io_object_t = 0;
pub const KERN_SUCCESS: kern_return_t = 0;
pub const kIOMainPortDefault: mach_port_t = 0;

pub static K_IO_GENERAL_INTEREST: &core::ffi::CStr = c"IOGeneralInterest";

pub type IOServiceInterestCallback = unsafe extern "C" fn(
    refcon: *mut c_void,
    service: io_service_t,
    messageType: natural_t,
    messageArgument: *mut c_void,
);

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOServiceMatching(name: *const std::os::raw::c_char) -> *const c_void;
    pub fn IOServiceGetMatchingService(
        mainPort: mach_port_t,
        matching: *const c_void,
    ) -> io_service_t;
    pub fn IORegistryEntryFromPath(
        mainPort: mach_port_t,
        path: *const std::os::raw::c_char,
    ) -> io_service_t;
    pub fn IORegistryEntryCreateCFProperty(
        entry: io_service_t,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *const c_void;
    pub fn IOObjectRelease(obj: io_object_t) -> kern_return_t;
    pub fn IONotificationPortCreate(mainPort: mach_port_t) -> *mut c_void;
    pub fn IONotificationPortGetRunLoopSource(notify: *mut c_void) -> *const c_void;
    pub fn IONotificationPortDestroy(notify: *mut c_void);
    pub fn IOServiceAddInterestNotification(
        notifyPort: *mut c_void,
        service: io_service_t,
        interestType: *const std::os::raw::c_char,
        callback: IOServiceInterestCallback,
        refcon: *mut c_void,
        notification: *mut io_object_t,
    ) -> kern_return_t;
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    pub fn CGGetActiveDisplayList(
        maxDisplays: u32,
        activeDisplays: *mut u32,
        displayCount: *mut u32,
    ) -> i32;
    pub fn CGDisplayIsBuiltin(display: u32) -> u32;
}

// Mirrors Swift:
//   err_system(0x38) | sub_iokit_pmu | 0x100
// where:
//   err_system(x) = (x & 0x3f) << 26
//   sub_iokit_pmu = err_sub(13) = (13 & 0xfff) << 14
pub const K_IOPM_MESSAGE_CLAMSHELL_STATE_CHANGE: natural_t = {
    let sys = (0x38u32 & 0x3f) << 26;
    let sub = (13u32 & 0xfff) << 14;
    sys | sub | 0x100
};

pub const K_CLAMSHELL_STATE_BIT: usize = 1;

// Power-source FFI for Task 19.
use core_foundation_sys::base::CFTypeRef;
use core_foundation_sys::string::CFStringRef;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOPSCopyPowerSourcesInfo() -> CFTypeRef;
    pub fn IOPSCopyPowerSourcesList(blob: CFTypeRef) -> CFTypeRef;
    pub fn IOPSGetProvidingPowerSourceType(blob: CFTypeRef) -> CFTypeRef;
    pub fn IOPSGetPowerSourceDescription(blob: CFTypeRef, ps: CFTypeRef) -> CFTypeRef;
    pub fn IOPSNotificationCreateRunLoopSource(
        callback: extern "C" fn(context: *mut std::ffi::c_void),
        context: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;
}

// IOPMAssertion FFI. Used to keep the display awake (idle-timer reset) without
// touching system sleep — the documented Apple mechanism for preventing
// the display from dimming and the screen from locking while a process is
// foregrounded.
// Note: assertion type strings (`kIOPMAssertPreventUserIdleDisplaySleep`) are
// passed as plain CFStrings; we construct them at the call site from a Rust
// string rather than dlsym'ing the framework's CFString constants.
pub type IOPMAssertionID = u32;
pub type IOPMAssertionLevel = u32;
pub const K_IOPM_ASSERTION_LEVEL_ON: IOPMAssertionLevel = 255;
pub const K_IO_RETURN_SUCCESS: IOReturn = 0;

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    pub fn IOPMAssertionCreateWithName(
        assertion_type: CFStringRef,
        assertion_level: IOPMAssertionLevel,
        assertion_name: CFStringRef,
        assertion_id: *mut IOPMAssertionID,
    ) -> IOReturn;
    pub fn IOPMAssertionRelease(assertion_id: IOPMAssertionID) -> IOReturn;
}
