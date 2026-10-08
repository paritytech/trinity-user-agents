//! Unified [`Scanner`] trait.

use crate::versioned::scanner::{
    HostScannerScanError, HostScannerScanRequest, HostScannerScanResponse,
};
use crate::{CallContext, CallError};
use crate::{wire, wire_trait};

/// QR codes and barcodes scanned through the host's own viewfinder.
///
/// The product receives the one code the user scanned, never camera frames,
/// so there is no permission to request: pointing the host's viewfinder at a
/// code is the consent. The host does not act on what it scanned, so a link
/// comes back as text. A product that needs the camera for anything else keeps
/// using `getUserMedia` under the `Camera` permission.
#[wire_trait(id = 25)]
#[crate::async_trait]
pub trait Scanner: Send + Sync {
    /// Ask the host to let the user scan one code.
    ///
    /// The host ignores codes outside `formats` or without `prefix` and keeps
    /// the viewfinder open. A host with no scanner answers `Unsupported`, and
    /// cancelling the call closes the viewfinder.
    ///
    /// ```ts
    /// const result = await truapi.scanner.scan({
    ///   formats: ["Qr"],
    ///   prefix: "https://greenmarket.example/r/",
    ///   hint: "Point at the receipt's QR code",
    /// });
    /// assert(result.isOk(), "scanner.scan failed:", result);
    /// const outcome = result.value.outcome;
    /// switch (outcome.tag) {
    ///   case "Scanned":
    ///     console.log("scanned:", outcome.value.format, outcome.value.text);
    ///     break;
    ///   case "Dismissed":
    ///     console.log("the user closed the viewfinder; worth offering again");
    ///     break;
    /// }
    /// ```
    #[wire(id = 0)]
    async fn scan(
        &self,
        _cx: &CallContext,
        _request: HostScannerScanRequest,
    ) -> Result<HostScannerScanResponse, CallError<HostScannerScanError>> {
        Err(CallError::unavailable())
    }
}
