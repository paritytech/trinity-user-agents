//! Shared resource controls for the test host.

use super::authority::AuthorityError;
use std::collections::HashSet;
use std::sync::{Mutex, atomic::AtomicBool};

/// Test-host allocation behavior shared by its wallet and product host.
#[derive(Default)]
pub struct TestResourceControls {
    unchecked: AtomicBool,
    withheld: Mutex<HashSet<String>>,
}

impl TestResourceControls {
    /// Whether allocation is answered as granted without performing it.
    pub fn grants_allowances_unchecked(&self) -> bool {
        self.unchecked.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Answer resource allocation as granted without performing it.
    pub fn set_grant_allowances_unchecked(&self, granted: bool) {
        self.unchecked
            .store(granted, std::sync::atomic::Ordering::Relaxed);
    }

    /// Replace refused resource tags; SmartContractAllowance covers every index.
    pub fn set_withheld_resources(&self, tags: Vec<String>) {
        *self
            .withheld
            .lock()
            .expect("withheld resource mutex poisoned") = tags.into_iter().collect();
    }

    /// Whether `resource` is answered as refused.
    pub fn withholds(&self, resource: &truapi::latest::AllocatableResource) -> bool {
        let tag = match resource {
            truapi::latest::AllocatableResource::StatementStoreAllowance => {
                "StatementStoreAllowance"
            }
            truapi::latest::AllocatableResource::BulletinAllowance => "BulletinAllowance",
            truapi::latest::AllocatableResource::SmartContractAllowance(_) => {
                "SmartContractAllowance"
            }
            truapi::latest::AllocatableResource::AutoSigning => "AutoSigning",
        };
        self.withheld
            .lock()
            .expect("withheld resource mutex poisoned")
            .contains(tag)
    }

    /// Withholding also applies to implicit native allowance access.
    pub fn refuse_withheld(
        &self,
        resource: &truapi::latest::AllocatableResource,
    ) -> Result<(), AuthorityError> {
        if self.withholds(resource) {
            return Err(AuthorityError::Rejected);
        }
        Ok(())
    }
}
