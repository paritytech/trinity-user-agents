//! Scanner host for the CLI. It has no camera, so every scan answers
//! `TRUAPI_SCAN_TEXT` as a QR code, or a dismissal when it is unset.

use truapi::latest::{CodeFormat, GenericError, HostScannerScanRequest};
use truapi::platform::{HostScan, ProductContext, ScannerPlatform, async_trait};

/// A scanner that answers every scan from the environment.
pub struct CliScannerHost;

#[async_trait]
impl ScannerPlatform for CliScannerHost {
    async fn scan_code(
        &self,
        product: &ProductContext,
        _request: &HostScannerScanRequest,
    ) -> Result<HostScan, GenericError> {
        let answer = match std::env::var("TRUAPI_SCAN_TEXT") {
            Ok(text) => HostScan::Scanned {
                text,
                format: CodeFormat::Qr,
            },
            Err(_) => HostScan::Dismissed,
        };
        tracing::info!(product = %product.product_id, ?answer, "scan answered");
        Ok(answer)
    }
}
