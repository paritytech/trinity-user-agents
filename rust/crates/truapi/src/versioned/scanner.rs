//! Versioned wrappers for [`Scanner`](crate::api::Scanner) methods.

use crate::v01;

truapi_macros::versioned_type! {
    pub enum HostScannerScanRequest { V1 => v01::HostScannerScanRequest }
    pub enum HostScannerScanResponse { V1 => v01::HostScannerScanResponse }
    pub enum HostScannerScanError { V1 => v01::HostScannerScanError }
}
