use parity_scale_codec::{Decode, Encode};

/// Code formats the host scanner reads: the set both platform decoders share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum CodeFormat {
    /// QR code.
    Qr,
    /// Aztec code.
    Aztec,
    /// Data Matrix code.
    DataMatrix,
    /// PDF417 code.
    Pdf417,
    /// EAN-13. A UPC-A code arrives as EAN-13 with a leading zero.
    Ean13,
    /// EAN-8.
    Ean8,
    /// UPC-E.
    UpcE,
    /// Code 128.
    Code128,
    /// Code 39.
    Code39,
    /// Code 93.
    Code93,
    /// Interleaved 2 of 5, including ITF-14.
    Itf,
}

/// Request to open the host's scanner.
///
/// The host draws the viewfinder and writes its title, naming the product.
/// Only `hint` is product text, shown as one plain line under the title.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct HostScannerScanRequest {
    /// Formats the product accepts. At least one.
    pub formats: Vec<CodeFormat>,
    /// Exact, case-sensitive start the text must have. At most 256 bytes.
    pub prefix: Option<String>,
    /// What to point the camera at. At most 80 characters, one line.
    pub hint: Option<String>,
}

/// How a scan ended.
///
/// A dismissal is an outcome rather than an error, because it is worth
/// offering again.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ScanOutcome {
    /// The user scanned a code the request accepts.
    Scanned {
        /// The code's content, as the platform decoder reports it.
        text: String,
        /// The code's format.
        format: CodeFormat,
    },
    /// The user closed the viewfinder without scanning.
    Dismissed,
}

/// Outcome of a scan.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostScannerScanResponse {
    /// How the scan ended.
    pub outcome: ScanOutcome,
}

/// Error returned by the scanner.
///
/// A host with no scanner answers `Unsupported` at the framework level rather
/// than through this enum.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostScannerScanError {
    /// The device has no camera, or the user refused the host application one.
    CameraUnavailable,
    /// Another scan is open.
    Busy,
    /// The request breaks a limit, so no viewfinder was shown.
    InvalidRequest {
        /// Which limit.
        reason: String,
    },
    /// Catch-all.
    Unknown {
        /// Human-readable reason.
        reason: String,
    },
}
