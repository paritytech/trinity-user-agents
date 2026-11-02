//! Product-facing scanner capability adapter.

use tracing::instrument;
use truapi::{
	CallContext, CallError,
	api::Scanner,
	latest,
	versioned::scanner::{HostScannerScanError, HostScannerScanRequest, HostScannerScanResponse},
};

use crate::{
	host_logic::scanner::{accepts, validate_request},
	platform::{ExecutableKind, HostScan},
	runtime::{ProductRuntimeHost, until_cancelled, versioning::version},
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
		version(request)
			.exec(|request| async move {
				let platform =
					self.connection.services.scanner_platform().ok_or(CallError::Unsupported)?;
				validate_request(&request).map_err(|reason| {
					CallError::Domain(latest::HostScannerScanError::InvalidRequest { reason })
				})?;
				if self.connection.execution_kind() == ExecutableKind::Worker &&
					!self.connection.recently_tapped()
				{
					return Err(CallError::Domain(latest::HostScannerScanError::NotVisible));
				}
				let _claim = self
					.connection
					.services
					.claim_scan()
					.ok_or_else(|| CallError::Domain(latest::HostScannerScanError::Busy))?;

				let answer =
					until_cancelled(cx, platform.scan_code(&self.connection.product, &request))
						.await
						.map_err(unknown)?
						.map_err(|error| unknown(error.reason))?;

				let outcome = match answer {
					HostScan::Scanned { text, format } if accepts(&request, format, &text) => {
						latest::ScanOutcome::Scanned { text, format }
					},
					HostScan::Scanned { .. } => {
						return Err(unknown(
							"the host returned a code the request does not accept",
						));
					},
					HostScan::Dismissed => latest::ScanOutcome::Dismissed,
					HostScan::CameraUnavailable => {
						return Err(CallError::Domain(
							latest::HostScannerScanError::CameraUnavailable,
						));
					},
					HostScan::NotVisible => {
						return Err(CallError::Domain(latest::HostScannerScanError::NotVisible));
					},
				};
				Ok(latest::HostScannerScanResponse { outcome })
			})
			.await
	}
}
