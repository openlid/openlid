//! External-display detection, force-display-sleep, and IOPMAssertion-based
//! prevent-display-sleep. The assertion is held per-process by this controller
//! and released either explicitly or when the process exits.

use super::iokit_ffi::{
    CGDisplayIsBuiltin, CGGetActiveDisplayList, IOPMAssertionCreateWithName, IOPMAssertionID,
    IOPMAssertionRelease, K_IOPM_ASSERTION_LEVEL_ON, K_IO_RETURN_SUCCESS,
};
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use openlid_core::platform::{DisplayController, PlatformError};
use std::process::Command;
use std::sync::Mutex;

pub struct MacDisplayController {
    // Holds the live assertion ID while we're keeping the display awake.
    // `None` = no assertion held. We don't store the ID across process
    // boundaries; macOS releases it automatically on process exit, so this
    // also serves as the natural crash-recovery story.
    assertion: Mutex<Option<IOPMAssertionID>>,
}

impl MacDisplayController {
    pub fn new() -> Self {
        Self {
            assertion: Mutex::new(None),
        }
    }
}

impl Default for MacDisplayController {
    fn default() -> Self {
        Self::new()
    }
}

impl DisplayController for MacDisplayController {
    fn has_external_display(&self) -> bool {
        let mut count: u32 = 0;
        let r = unsafe { CGGetActiveDisplayList(0, std::ptr::null_mut(), &mut count) };
        if r != 0 || count == 0 {
            return false;
        }
        let mut ids = vec![0u32; count as usize];
        let r = unsafe { CGGetActiveDisplayList(count, ids.as_mut_ptr(), &mut count) };
        if r != 0 {
            return false;
        }
        ids.iter()
            .take(count as usize)
            .any(|d| unsafe { CGDisplayIsBuiltin(*d) } == 0)
    }

    fn force_display_sleep(&self) -> Result<(), PlatformError> {
        let out = Command::new("/usr/bin/pmset")
            .arg("displaysleepnow")
            .output()
            .map_err(PlatformError::Io)?;
        if !out.status.success() {
            return Err(PlatformError::Native(format!(
                "pmset displaysleepnow failed: {}",
                String::from_utf8_lossy(&out.stderr)
            )));
        }
        Ok(())
    }

    fn prevent_display_sleep(&self) -> Result<(), PlatformError> {
        let mut guard = self.assertion.lock().unwrap();
        if guard.is_some() {
            return Ok(()); // already held — idempotent
        }
        let assertion_type = CFString::new("PreventUserIdleDisplaySleep");
        let assertion_name = CFString::new("io.openlid.app: keep display awake");
        let mut id: IOPMAssertionID = 0;
        let r = unsafe {
            IOPMAssertionCreateWithName(
                assertion_type.as_concrete_TypeRef(),
                K_IOPM_ASSERTION_LEVEL_ON,
                assertion_name.as_concrete_TypeRef(),
                &mut id,
            )
        };
        if r != K_IO_RETURN_SUCCESS {
            return Err(PlatformError::Native(format!(
                "IOPMAssertionCreateWithName failed: {r:#x}"
            )));
        }
        *guard = Some(id);
        tracing::debug!("acquired PreventUserIdleDisplaySleep assertion id={id}");
        Ok(())
    }

    fn allow_display_sleep(&self) -> Result<(), PlatformError> {
        let mut guard = self.assertion.lock().unwrap();
        let Some(id) = guard.take() else {
            return Ok(()); // not held — idempotent
        };
        let r = unsafe { IOPMAssertionRelease(id) };
        if r != K_IO_RETURN_SUCCESS {
            // Re-park the id so a retry can release it, then surface the error.
            *guard = Some(id);
            return Err(PlatformError::Native(format!(
                "IOPMAssertionRelease failed: {r:#x}"
            )));
        }
        tracing::debug!("released PreventUserIdleDisplaySleep assertion id={id}");
        Ok(())
    }
}
