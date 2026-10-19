//! Product-facing share sheet capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::Share,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		share::{HostShareError, HostShareRequest, HostShareResponse},
	},
};

use super::from_latest_error;
use crate::{
	platform::{HostShare, ProductExecutionKind},
	runtime::{ProductRuntimeHost, until_cancelled},
};

/// Largest text a product may share, in bytes of UTF-8.
pub const SHARE_TEXT_MAX_BYTES: usize = 8 * 1024;

fn unknown(reason: impl ToString) -> CallError<latest::HostShareError> {
	CallError::Domain(latest::HostShareError::Unknown { reason: reason.to_string() })
}

/// The reason `request` breaks a limit, or `None` when the host may show it.
fn invalid_reason(request: &latest::HostShareRequest) -> Option<String> {
	let text = &request.text;
	if text.trim().is_empty() {
		return Some("the text is empty".to_string());
	}
	if text.len() > SHARE_TEXT_MAX_BYTES {
		return Some(format!("the text is longer than {SHARE_TEXT_MAX_BYTES} bytes"));
	}
	text.chars()
		.any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
		.then(|| "the text has a control character".to_string())
}

#[truapi::async_trait]
impl Share for ProductRuntimeHost {
	/// Checked in this order so a host without a share sheet, or a request
	/// the product could not make, never opens a sheet.
	#[instrument(skip_all, fields(runtime.method = "share.share"))]
	async fn share(
		&self,
		cx: &CallContext,
		request: HostShareRequest,
	) -> Result<HostShareResponse, CallError<HostShareError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform =
				self.connection.services.share_platform().ok_or(CallError::Unsupported)?;
			if let Some(reason) = invalid_reason(&request) {
				return Err(CallError::Domain(latest::HostShareError::InvalidRequest { reason }));
			}
			if self.connection.execution_kind() == ProductExecutionKind::Worker &&
				!self.connection.recently_pressed()
			{
				return Err(CallError::Domain(latest::HostShareError::NotVisible));
			}
			let _claim = self
				.connection
				.services
				.claim_share()
				.ok_or_else(|| CallError::Domain(latest::HostShareError::Busy))?;
			let answer =
				until_cancelled(cx, platform.share_text(&self.connection.product, &request))
					.await
					.map_err(unknown)?
					.map_err(|error| unknown(error.reason))?;
			let outcome = match answer {
				HostShare::Shared => latest::ShareOutcome::Shared,
				HostShare::Dismissed => latest::ShareOutcome::Dismissed,
				HostShare::NotVisible => {
					return Err(CallError::Domain(latest::HostShareError::NotVisible));
				},
			};
			Ok(latest::HostShareResponse { outcome })
		}
		.await;
		result
			.map(|response| HostShareResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn request(text: &str) -> latest::HostShareRequest {
		latest::HostShareRequest { text: text.to_string() }
	}

	#[test]
	fn a_text_with_a_link_and_line_breaks_may_be_shared() {
		assert_eq!(
			invalid_reason(&request("Add me:\r\nhttps://verbosity.dot.li/-/chat/x#y\t!")),
			None
		);
	}

	#[test]
	fn an_empty_long_or_control_text_is_refused() {
		assert!(invalid_reason(&request("  \n")).is_some());
		assert!(invalid_reason(&request(&"x".repeat(SHARE_TEXT_MAX_BYTES + 1))).is_some());
		assert_eq!(invalid_reason(&request(&"x".repeat(SHARE_TEXT_MAX_BYTES))), None);
		assert!(invalid_reason(&request("bell \u{7}")).is_some());
	}
}
