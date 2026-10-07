//! Scanner host for the CLI.
//!
//! A CLI has no camera, so the code it scans comes from `TRUAPI_SCAN_TEXT`,
//! read on every scan and reported as a QR code. With nothing set the user
//! dismisses, which lets the generated example and the battery pass on a bare
//! host. The host does not filter: the core refuses a configured code the
//! request does not accept.

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
        let answer = answer(std::env::var("TRUAPI_SCAN_TEXT").ok());
        tracing::info!(product = %product.product_id, ?answer, "scan answered");
        Ok(answer)
    }
}

fn answer(text: Option<String>) -> HostScan {
    match text {
        Some(text) => HostScan::Scanned {
            text,
            format: CodeFormat::Qr,
        },
        None => HostScan::Dismissed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn with_nothing_set_the_user_dismisses() {
        // The generated example and the battery run on a bare CLI.
        assert_eq!(answer(None), HostScan::Dismissed);
    }

    #[test]
    fn a_set_text_is_scanned_as_a_qr_code() {
        assert_eq!(
            answer(Some("https://greenmarket.example/r/1".into())),
            HostScan::Scanned {
                text: "https://greenmarket.example/r/1".into(),
                format: CodeFormat::Qr,
            }
        );
    }
}
