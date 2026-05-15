//! Wraps IOPowerSources to read current power source and subscribe to changes.

use super::iokit_ffi::*;
use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::dictionary::CFDictionary;
use core_foundation::number::CFNumber;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoopAddSource, CFRunLoopGetMain};
use core_foundation::string::CFString;
use openlid_core::mode::PowerSource;
use openlid_core::platform::{PowerSourceCallback, PowerSourceMonitor};
use std::sync::{Arc, Mutex};

pub struct MacPowerSourceMonitor {
    inner: Arc<Mutex<Inner>>,
}

struct Inner {
    callback: Option<PowerSourceCallback>,
}

unsafe impl Send for Inner {}

impl MacPowerSourceMonitor {
    pub fn start() -> anyhow::Result<Self> {
        let inner = Arc::new(Mutex::new(Inner { callback: None }));
        let refcon = Arc::into_raw(Arc::clone(&inner)) as *mut std::ffi::c_void;
        let src = unsafe { IOPSNotificationCreateRunLoopSource(Self::on_change, refcon) };
        if src.is_null() {
            anyhow::bail!("IOPSNotificationCreateRunLoopSource returned null");
        }
        unsafe {
            CFRunLoopAddSource(CFRunLoopGetMain(), src as *mut _, kCFRunLoopCommonModes);
        }
        Ok(Self { inner })
    }

    fn read_current() -> PowerSource {
        let blob = unsafe { IOPSCopyPowerSourcesInfo() };
        if blob.is_null() {
            return PowerSource::Ac;
        }
        let type_ref = unsafe { IOPSGetProvidingPowerSourceType(blob) };
        if type_ref.is_null() {
            return PowerSource::Ac;
        }
        let kind = unsafe { CFString::wrap_under_get_rule(type_ref as *const _) }.to_string();
        let is_battery = kind.contains("Battery");

        let mut percent: u8 = 100;
        if is_battery {
            let list = unsafe { IOPSCopyPowerSourcesList(blob) };
            if !list.is_null() {
                let arr: CFArray = unsafe { CFArray::wrap_under_create_rule(list as *const _) };
                if let Some(ps) = arr.get(0) {
                    let desc_ref = unsafe {
                        IOPSGetPowerSourceDescription(
                            blob,
                            *ps as core_foundation_sys::base::CFTypeRef,
                        )
                    };
                    if !desc_ref.is_null() {
                        let dict: CFDictionary =
                            unsafe { CFDictionary::wrap_under_get_rule(desc_ref as *const _) };
                        let key = CFString::new("Current Capacity");
                        if let Some(v) = dict.find(key.as_concrete_TypeRef() as *const _) {
                            let n = unsafe { CFNumber::wrap_under_get_rule(*v as *const _) };
                            if let Some(i) = n.to_i32() {
                                percent = i.clamp(0, 100) as u8;
                            }
                        }
                    }
                }
            }
        }
        // NOTE: blob is leaked. Per-call leak is ~hundreds of bytes; OK for MVP.

        if is_battery {
            PowerSource::Battery { percent }
        } else {
            PowerSource::Ac
        }
    }

    extern "C" fn on_change(context: *mut std::ffi::c_void) {
        if context.is_null() {
            return;
        }
        let inner = unsafe { Arc::from_raw(context as *const Mutex<Inner>) };
        let cb = inner.lock().unwrap().callback.clone();
        std::mem::forget(inner);
        if let Some(cb) = cb {
            cb(Self::read_current());
        }
    }
}

impl PowerSourceMonitor for MacPowerSourceMonitor {
    fn current(&self) -> PowerSource {
        Self::read_current()
    }

    fn subscribe(&self, callback: PowerSourceCallback) {
        self.inner.lock().unwrap().callback = Some(callback);
    }
}
