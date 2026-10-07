//! Product-facing scanner capability adapter.

use tracing::instrument;
use truapi::api::Scanner;
use truapi::latest;
use truapi::versioned::scanner::{
    HostScannerScanError, HostScannerScanRequest, HostScannerScanResponse,
};
use truapi::{CallContext, CallError};

use crate::host_logic::scanner::{accepts, validate_request};
use crate::platform::HostScan;
use crate::runtime::{ProductRuntimeHost, until_cancelled};

fn domain(error: latest::HostScannerScanError) -> CallError<HostScannerScanError> {
    CallError::Domain(HostScannerScanError::V1(error))
}

fn unknown(reason: impl ToString) -> CallError<HostScannerScanError> {
    domain(latest::HostScannerScanError::Unknown {
        reason: reason.to_string(),
    })
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
        let HostScannerScanRequest::V1(request) = request;
        let platform = self
            .services
            .scanner_platform()
            .ok_or(CallError::Unsupported)?;
        validate_request(&request)
            .map_err(|reason| domain(latest::HostScannerScanError::InvalidRequest { reason }))?;
        let _claim = self
            .services
            .claim_scan()
            .ok_or_else(|| domain(latest::HostScannerScanError::Busy))?;

        let answer = until_cancelled(cx, platform.scan_code(&self.product, &request))
            .await
            .map_err(unknown)?
            .map_err(|error| unknown(error.reason))?;

        let outcome = match answer {
            HostScan::Scanned { text, format } if accepts(&request, format, &text) => {
                latest::ScanOutcome::Scanned { text, format }
            }
            HostScan::Scanned { .. } => {
                return Err(unknown(
                    "the host returned a code the request does not accept",
                ));
            }
            HostScan::Dismissed => latest::ScanOutcome::Dismissed,
            HostScan::CameraUnavailable => {
                return Err(domain(latest::HostScannerScanError::CameraUnavailable));
            }
        };
        Ok(HostScannerScanResponse::V1(
            latest::HostScannerScanResponse { outcome },
        ))
    }
}
