//! Subscription-interrupt helpers shared by the host-facing bridges.

use truapi::CallError;

/// Render the value a subscription ended with as the diagnostic string a
/// host-side observer reports. Bridges that hand an interrupt to a native or
/// JavaScript callback have only a string to give it.
///
/// `CallError<GenericError>` is the interrupt type the custom-renderer
/// bridges that call this declare, and a versioned domain wrapper has no
/// `Display`, so there is nothing for a generic version to render.
pub fn interrupt_reason(error: CallError<truapi::latest::GenericError>) -> String {
	match error {
		CallError::Domain(truapi::latest::GenericError { reason }) => reason,
		CallError::Denied => "denied".to_string(),
		CallError::Unsupported => "unsupported".to_string(),
		CallError::MalformedFrame { reason } | CallError::HostFailure { reason } => reason,
		CallError::Cancelled => "cancelled".to_string(),
	}
}

/// Strip a subscription interrupt's versioned envelope, leaving the latest
/// domain payload. Framework variants carry no version and pass through.
pub fn interrupt_into_latest<E>(error: CallError<E>) -> CallError<E::Latest>
where
	E: truapi::versioned::IntoLatest,
{
	match error {
		CallError::Domain(domain) => CallError::Domain(domain.into_latest()),
		CallError::Denied => CallError::Denied,
		CallError::Unsupported => CallError::Unsupported,
		CallError::MalformedFrame { reason } => CallError::MalformedFrame { reason },
		CallError::HostFailure { reason } => CallError::HostFailure { reason },
		CallError::Cancelled => CallError::Cancelled,
	}
}
