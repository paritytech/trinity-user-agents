//! Versioned wrappers for [`Profile`](crate::api::Profile) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostProfilePresentRequest { V1 => v01::HostProfilePresentRequest }
    pub enum HostProfilePresentResponse { V1 }
    pub enum HostProfilePresentError { V1 => v01::HostProfilePresentError }
}
