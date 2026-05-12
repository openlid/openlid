//! External-display detection and force-display-sleep.

use super::iokit_ffi::{CGDisplayIsBuiltin, CGGetActiveDisplayList};
use open_lid_core::platform::{DisplayController, PlatformError};
use std::process::Command;

pub struct MacDisplayController;

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
}
