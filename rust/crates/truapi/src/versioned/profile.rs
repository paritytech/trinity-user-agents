//! Versioned wrappers for [`Profile`](crate::api::Profile) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostProfilePresentRequest { V1 => v01::HostProfilePresentRequest }
    pub enum HostProfilePresentResponse { V1 }
    pub enum HostProfilePresentError { V1 => v01::HostProfilePresentError }
    pub enum HostProfileDiscloseRequest { V1 => v01::HostProfileDiscloseRequest }
    pub enum HostProfileDiscloseResponse { V1 }
    pub enum HostProfileDiscloseError { V1 => v01::HostProfileDiscloseError }
    pub enum HostProfileRetractRequest { V1 }
    pub enum HostProfileRetractResponse { V1 }
    pub enum HostProfileRetractError { V1 => v01::HostProfileRetractError }
    pub enum HostProfilePresentContactRequest { V1 => v01::HostProfilePresentContactRequest }
    pub enum HostProfilePresentContactResponse { V1 }
    pub enum HostProfilePresentContactError { V1 => v01::HostProfilePresentContactError }
}
