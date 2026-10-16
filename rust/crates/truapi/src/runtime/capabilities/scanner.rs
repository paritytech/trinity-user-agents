//! Product-facing scanner capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::Scanner,
	latest,
	versioned::{
		FromLatest, IntoLatest, Versioned,
		scanner::{HostScannerScanError, HostScannerScanRequest, HostScannerScanResponse},
	},
};

use super::from_latest_error;
use crate::{
	host_logic::scanner::{accepts, validate_request},
	platform::{HostScan, ProductExecutionKind},
	runtime::{ProductRuntimeHost, until_cancelled},
};

fn unknown(reason: impl ToString) -> CallError<latest::HostScannerScanError> {
	CallError::Domain(latest::HostScannerScanError::Unknown { reason: reason.to_string() })
}

#[truapi::async_trait]
impl Scanner for ProductRuntimeHost {
	/// Checked in this order so a host without a scanner, or a request the
	/// product could not make, never opens a viewfinder.
	#[instrument(skip_all, fields(runtime.method = "scanner.scan"))]
	async fn scan(
		&self,
		cx: &CallContext,
		request: HostScannerScanRequest,
	) -> Result<HostScannerScanResponse, CallError<HostScannerScanError>> {
		let version = request.version();
		let request = request.into_latest();
		let result = async {
			let platform = self.services.scanner_platform().ok_or(CallError::Unsupported)?;
			validate_request(&request).map_err(|reason| {
				CallError::Domain(latest::HostScannerScanError::InvalidRequest { reason })
			})?;
			if self.execution_kind() == ProductExecutionKind::Worker && !self.recently_tapped() {
				return Err(CallError::Domain(latest::HostScannerScanError::NotVisible));
			}
			let _claim = self
				.services
				.claim_scan()
				.ok_or_else(|| CallError::Domain(latest::HostScannerScanError::Busy))?;

			let answer = until_cancelled(cx, platform.scan_code(&self.product, &request))
				.await
				.map_err(unknown)?
				.map_err(|error| unknown(error.reason))?;

			let outcome = match answer {
				HostScan::Scanned { text, format } if accepts(&request, format, &text) => {
					latest::ScanOutcome::Scanned { text, format }
				},
				HostScan::Scanned { .. } => {
					return Err(unknown("the host returned a code the request does not accept"));
				},
				HostScan::Dismissed => latest::ScanOutcome::Dismissed,
				HostScan::CameraUnavailable => {
					return Err(CallError::Domain(latest::HostScannerScanError::CameraUnavailable));
				},
				HostScan::NotVisible => {
					return Err(CallError::Domain(latest::HostScannerScanError::NotVisible));
				},
			};
			Ok(latest::HostScannerScanResponse { outcome })
		}
		.await;
		result
			.map(|response| HostScannerScanResponse::from_latest(response, version))
			.map_err(|error| from_latest_error(error, version))
	}
}
