//! Monitors lid open/closed state via IOKit IOPMrootDomain.
//! Port of Sources/Modafinil/LidMonitor.swift.

#![allow(dead_code)]

use super::iokit_ffi::*;
use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::runloop::{CFRunLoopAddSource, CFRunLoopGetMain, kCFRunLoopCommonModes};
use core_foundation::string::CFString;
use open_lid_core::mode::LidState;
use open_lid_core::platform::{LidObserver, LidStateCallback};
use std::ffi::CString;
use std::sync::{Arc, Mutex};

pub struct MacLidMonitor {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    root_domain: io_service_t,
    notification_port: *mut std::ffi::c_void,
    notifier: io_object_t,
    callback: Option<LidStateCallback>,
}

unsafe impl Send for Inner {}

impl MacLidMonitor {
    pub fn start() -> anyhow::Result<Self> {
        let root_domain = unsafe { find_root_domain() };
        if root_domain == IO_OBJECT_NULL {
            anyhow::bail!("IOPMrootDomain not found");
        }
        let port = unsafe { IONotificationPortCreate(kIOMainPortDefault) };
        if port.is_null() {
            unsafe { IOObjectRelease(root_domain); }
            anyhow::bail!("IONotificationPortCreate returned null");
        }
        let source = unsafe { IONotificationPortGetRunLoopSource(port) };
        unsafe {
            CFRunLoopAddSource(
                CFRunLoopGetMain(),
                source as *mut _,
                kCFRunLoopCommonModes,
            );
        }

        let inner = Arc::new(Mutex::new(Inner {
            root_domain,
            notification_port: port,
            notifier: IO_OBJECT_NULL,
            callback: None,
        }));
        let refcon = Arc::into_raw(Arc::clone(&inner)) as *mut std::ffi::c_void;

        let mut notifier: io_object_t = IO_OBJECT_NULL;
        let kr = unsafe {
            IOServiceAddInterestNotification(
                port,
                root_domain,
                K_IO_GENERAL_INTEREST.as_ptr(),
                Self::on_message,
                refcon,
                &mut notifier,
            )
        };
        if kr != KERN_SUCCESS {
            anyhow::bail!("IOServiceAddInterestNotification failed: {kr}");
        }
        inner.lock().unwrap().notifier = notifier;

        Ok(Self { inner })
    }

    pub fn read_current() -> LidState {
        let root_domain = unsafe { find_root_domain() };
        if root_domain == IO_OBJECT_NULL {
            return LidState::Open;
        }
        let key = CFString::new("AppleClamshellState");
        let cf_ptr = unsafe {
            IORegistryEntryCreateCFProperty(
                root_domain,
                key.as_concrete_TypeRef() as *const _,
                std::ptr::null(),
                0,
            )
        };
        unsafe { IOObjectRelease(root_domain); }
        if cf_ptr.is_null() {
            return LidState::Open;
        }
        let cf = unsafe { CFBoolean::wrap_under_create_rule(cf_ptr as *const _) };
        if cf.into() { LidState::Closed } else { LidState::Open }
    }

    unsafe extern "C" fn on_message(
        refcon: *mut std::ffi::c_void,
        _service: io_service_t,
        message_type: natural_t,
        message_argument: *mut std::ffi::c_void,
    ) {
        if refcon.is_null() {
            return;
        }
        if message_type != K_IOPM_MESSAGE_CLAMSHELL_STATE_CHANGE {
            return;
        }
        let bits = message_argument as usize;
        let closed = (bits & K_CLAMSHELL_STATE_BIT) != 0;
        let state = if closed { LidState::Closed } else { LidState::Open };

        let inner = unsafe { Arc::from_raw(refcon as *const Mutex<Inner>) };
        let cb = inner.lock().unwrap().callback.clone();
        std::mem::forget(inner);
        if let Some(cb) = cb {
            cb(state);
        }
    }
}

impl Drop for MacLidMonitor {
    fn drop(&mut self) {
        let mut inner = self.inner.lock().unwrap();
        if inner.notifier != IO_OBJECT_NULL {
            unsafe { IOObjectRelease(inner.notifier); }
            inner.notifier = IO_OBJECT_NULL;
        }
        if !inner.notification_port.is_null() {
            unsafe { IONotificationPortDestroy(inner.notification_port); }
            inner.notification_port = std::ptr::null_mut();
        }
        if inner.root_domain != IO_OBJECT_NULL {
            unsafe { IOObjectRelease(inner.root_domain); }
            inner.root_domain = IO_OBJECT_NULL;
        }
    }
}

impl LidObserver for MacLidMonitor {
    fn current(&self) -> LidState {
        Self::read_current()
    }

    fn subscribe(&self, callback: LidStateCallback) {
        self.inner.lock().unwrap().callback = Some(callback);
    }
}

unsafe fn find_root_domain() -> io_service_t {
    let name = CString::new("IOPMrootDomain").unwrap();
    let matched = unsafe { IOServiceGetMatchingService(kIOMainPortDefault, IOServiceMatching(name.as_ptr())) };
    if matched != IO_OBJECT_NULL {
        return matched;
    }
    let path = CString::new("IOService:/IOResources/IOPowerConnection/IOPMrootDomain").unwrap();
    unsafe { IORegistryEntryFromPath(kIOMainPortDefault, path.as_ptr()) }
}
