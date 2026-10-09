//! Product-facing scanner capability adapter.

use tracing::instrument;
use truapi::api::Scanner;
use truapi::versioned::scanner::{
    HostScannerScanError, HostScannerScanRequest, HostScannerScanResponse,
};
use truapi::{CallContext, CallError};

use crate::runtime::ProductRuntimeHost;

#[truapi::async_trait]
impl Scanner for ProductRuntimeHost {
    #[instrument(skip_all, fields(runtime.method = "scanner.scan"))]
    async fn scan(
        &self,
        _cx: &CallContext,
        _request: HostScannerScanRequest,
    ) -> Result<HostScannerScanResponse, CallError<HostScannerScanError>> {
        Err(CallError::Unsupported)
    }
}
