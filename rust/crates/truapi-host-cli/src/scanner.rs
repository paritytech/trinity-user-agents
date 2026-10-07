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
    fn answers_the_configured_text_or_a_dismissal() {
        // A bare CLI dismisses, so the generated example and the battery pass.
        assert_eq!(answer(None), HostScan::Dismissed);
        assert_eq!(
            answer(Some("https://greenmarket.example/r/1".into())),
            HostScan::Scanned {
                text: "https://greenmarket.example/r/1".into(),
                format: CodeFormat::Qr,
            }
        );
    }
}
