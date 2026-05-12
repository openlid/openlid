//! Validates that an incoming XPC client is signed with the expected
//! bundle identifier and Team ID. Uses Security framework SecCode APIs.

use anyhow::{anyhow, Result};
use core_foundation::base::TCFType;
use core_foundation::data::CFData;
use core_foundation::dictionary::CFDictionary;
use core_foundation::string::CFString;
use security_framework_sys::base::errSecSuccess;
use security_framework_sys::code_signing::{
    kSecGuestAttributeAudit, SecCodeCheckValidity, SecCodeCopyGuestWithAttributes, SecCodeRef,
    SecRequirementCreateWithString, SecRequirementRef,
};

pub struct ClientValidator {
    requirement_text: String,
}

impl ClientValidator {
    /// Build a validator with a code-requirement string.
    /// Plan 1 (dev) requirement: `identifier "io.openlid.app"`
    /// Plan 2 (prod) requirement adds Team ID pinning.
    pub fn new(requirement_text: impl Into<String>) -> Self {
        Self {
            requirement_text: requirement_text.into(),
        }
    }

    pub fn allows(&self, audit_token: [u8; 32]) -> bool {
        match self.try_allows(audit_token) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("client validation error: {e:#}");
                false
            }
        }
    }

    fn try_allows(&self, audit_token: [u8; 32]) -> Result<bool> {
        let token_data = CFData::from_buffer(&audit_token);

        // SAFETY: kSecGuestAttributeAudit is a valid CFStringRef exported by the Security framework.
        let token_key = unsafe { CFString::wrap_under_get_rule(kSecGuestAttributeAudit) };

        let attrs =
            CFDictionary::from_CFType_pairs(&[(token_key.as_CFType(), token_data.as_CFType())]);

        let mut guest: SecCodeRef = std::ptr::null_mut();
        // SAFETY: SecCodeCopyGuestWithAttributes is a valid Security.framework function.
        let status = unsafe {
            SecCodeCopyGuestWithAttributes(
                std::ptr::null_mut(),
                attrs.as_concrete_TypeRef(),
                0,
                &mut guest,
            )
        };
        if status != errSecSuccess || guest.is_null() {
            return Err(anyhow!("SecCodeCopyGuestWithAttributes failed: {status}"));
        }

        let req_text_cf = CFString::new(&self.requirement_text);
        let mut req: SecRequirementRef = std::ptr::null_mut();
        // SAFETY: SecRequirementCreateWithString is a valid Security.framework function.
        let status = unsafe {
            SecRequirementCreateWithString(req_text_cf.as_concrete_TypeRef(), 0, &mut req)
        };
        if status != errSecSuccess || req.is_null() {
            // SAFETY: guest was successfully created above; we must release it.
            unsafe { core_foundation_sys::base::CFRelease(guest as *const _) };
            return Err(anyhow!("SecRequirementCreateWithString failed: {status}"));
        }

        // SAFETY: both guest and req are valid non-null refs created above.
        let check = unsafe { SecCodeCheckValidity(guest, 0, req) };
        unsafe {
            core_foundation_sys::base::CFRelease(guest as *const _);
            core_foundation_sys::base::CFRelease(req as *const _);
        }
        Ok(check == errSecSuccess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validator_with_invalid_token_rejects() {
        let v = ClientValidator::new(r#"identifier "io.openlid.app""#);
        let bogus = [0u8; 32];
        assert!(!v.allows(bogus));
    }
}
