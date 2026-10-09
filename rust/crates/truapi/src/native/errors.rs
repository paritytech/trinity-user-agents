use truapi::v01;



/// Native-friendly rejection error returned by callback methods that map onto
/// [`truapi::v01::GenericError`].
///
/// [`uniffi::Error` is the value-style error mapping and only supports enums;
/// UniFFI's struct alternative is an `Arc`-backed object
/// error](https://mozilla.github.io/uniffi-rs/0.32/types/errors.html). Making the
/// canonical SCALE value an object would require Rust-owned handles and foreign
/// construction solely to carry one string. This local enum keeps the native
/// exception value-like without changing the canonical wire representation.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum HostRejection {
    /// Caller rejected the operation.
    #[error("{reason}")]
    Rejected {
        /// Human-readable rejection reason.
        reason: String,
    },
}

impl From<HostRejection> for v01::GenericError {
    fn from(err: HostRejection) -> Self {
        let HostRejection::Rejected { reason } = err;
        v01::GenericError { reason }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for HostRejection {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        tracing::warn!(
            reason = %err.reason,
            "host callback threw an undeclared error; reporting it as a rejection"
        );
        HostRejection::Rejected { reason: err.reason }
    }
}

// Foreign callbacks that throw an undeclared exception land here; without these
// conversions UniFFI panics, which `panic = "abort"` turns into a process abort.
impl From<uniffi::UnexpectedUniFFICallbackError> for v01::HostLocalStorageReadError {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        tracing::warn!(
            reason = %err.reason,
            "host callback threw an undeclared error; reporting it as a rejection"
        );
        v01::HostLocalStorageReadError::Unknown { reason: err.reason }
    }
}

impl From<uniffi::UnexpectedUniFFICallbackError> for v01::HostNavigateToError {
    fn from(err: uniffi::UnexpectedUniFFICallbackError) -> Self {
        tracing::warn!(
            reason = %err.reason,
            "host callback threw an undeclared error; reporting it as a rejection"
        );
        v01::HostNavigateToError::Unknown { reason: err.reason }
    }
}

impl From<v01::GenericError> for HostRejection {
    fn from(err: v01::GenericError) -> Self {
        HostRejection::Rejected { reason: err.reason }
    }
}

/// Why the core database status could not be read.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativeCoreDatabaseError {
    /// The database could not be read.
    #[error("core database unavailable: {reason}")]
    Unavailable {
        /// What failed.
        reason: String,
    },
}

impl From<crate::store::DbError> for NativeCoreDatabaseError {
    fn from(error: crate::store::DbError) -> Self {
        Self::Unavailable {
            reason: error.to_string(),
        }
    }
}
