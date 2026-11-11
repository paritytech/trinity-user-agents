use truapi::v01;

use crate::platform::ChatFieldError;

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

/// Maps an undeclared foreign exception onto each error's own unknown
/// variant.
///
/// Foreign callbacks that throw an undeclared exception land here; without
/// these conversions UniFFI panics, which `panic = "abort"` turns into a
/// process abort. The host's own message is bounded before it is kept, so
/// every later line that renders this error, and the product it is returned
/// to, get a string that is already one line and already short. A conversion
/// added later carries that without being asked to.
///
/// The variant is named rather than written out, so take care adding a target
/// whose unknown variant is not the only one carrying a `reason`.
macro_rules! from_undeclared_host_error {
	($($target:ty => $variant:ident),+ $(,)?) => {
		$(
			impl From<::uniffi::UnexpectedUniFFICallbackError> for $target {
				fn from(err: ::uniffi::UnexpectedUniFFICallbackError) -> Self {
					::tracing::warn!(
						error = stringify!($target),
						reason = %$crate::logging::HostText::new(&err.reason),
						"host callback threw an undeclared error; reporting it as a rejection"
					);
					Self::$variant { reason: $crate::logging::bound_host_text(&err.reason) }
				}
			}
		)+
	};
}

from_undeclared_host_error! {
	HostRejection => Rejected,
	v01::HostLocalStorageReadError => Unknown,
	crate::latest::HostPaymentError => Unknown,
	crate::latest::HostPaymentTopUpError => Unknown,
	crate::latest::HostPaymentBalanceSubscribeError => Unknown,
	crate::latest::HostNavigateToError => Unknown,
}

impl From<uniffi::UnexpectedUniFFICallbackError> for crate::latest::HostContactsPlaceLabelsError {
	fn from(_: uniffi::UnexpectedUniFFICallbackError) -> Self {
		Self::Unknown { reason: "contact label callback failed".into() }
	}
}

impl From<v01::GenericError> for HostRejection {
	fn from(err: v01::GenericError) -> Self {
		HostRejection::Rejected { reason: err.reason }
	}
}

impl From<crate::latest::HostNotificationReceivingError> for HostRejection {
	fn from(error: crate::latest::HostNotificationReceivingError) -> Self {
		Self::Rejected { reason: format!("background receiving: {error:?}") }
	}
}

/// Rejection of a card face, read from its declared JSON or from the bytes a
/// host kept.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativeRendererError {
	/// The text is not a renderer tree in the shape the protocol describes.
	#[error("{reason}")]
	Malformed {
		/// Why the text could not be read.
		reason: String,
	},
	/// The tree nests deeper than any host will draw.
	#[error("renderer tree nests deeper than {limit} levels")]
	TooDeep {
		/// Largest accepted nesting.
		limit: u32,
	},
	/// The thread faces are read on could not be started, so the face was not
	/// read at all. Reading it again may succeed.
	#[error("face reader could not start: {reason}")]
	ReaderUnavailable {
		/// Why the thread could not be started.
		reason: String,
	},
}

/// Rejection of a product-supplied chat field, value-shaped for UniFFI.
///
/// [`ChatFieldError`] names its field with a `&'static str`, which has no
/// UniFFI representation, so the field name travels inside the message.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum NativeChatFieldError {
	/// The field was refused; `reason` names the field and why.
	#[error("{reason}")]
	Refused {
		/// Which field was refused, and why.
		reason: String,
	},
}

impl From<ChatFieldError> for NativeChatFieldError {
	fn from(err: ChatFieldError) -> Self {
		Self::Refused { reason: err.to_string() }
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
		Self::Unavailable { reason: error.to_string() }
	}
}
