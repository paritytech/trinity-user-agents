use derive_more::Display;
use parity_scale_codec::{Decode, Encode};

/// Device-capability permission requested from the host (RFC 0002).
///
/// Lasting grants and denials survive app restarts. A host may also offer a
/// one-use grant, held in memory until a permission-gated operation consumes it.
///
/// That decision is about this product. The OS grant behind it belongs to the
/// host application and can move independently, so a host that can read OS
/// state has the capability resolve only while both allow it: a stored grant
/// whose OS grant was revoked answers `granted: false` without a prompt. An OS
/// grant that is merely undetermined does not change the answer, because the OS
/// resolves its own gate when the capability is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, Display)]
#[allow(clippy::upper_case_acronyms)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum HostDevicePermissionRequest {
    /// Showing system notifications.
    #[display("notifications")]
    Notifications,
    /// Camera capture access.
    #[display("camera")]
    Camera,
    /// Microphone capture access.
    #[display("microphone")]
    Microphone,
    /// Bluetooth device access.
    #[display("bluetooth")]
    Bluetooth,
    /// NFC reader access.
    #[display("NFC")]
    NFC,
    /// Geolocation access.
    #[display("location")]
    Location,
    /// Clipboard access.
    #[display("clipboard")]
    Clipboard,
    /// Opening an external URL through `navigate_to`. A one-use grant allows
    /// one handoff to the browser or another system application.
    #[display("open URL")]
    OpenUrl,
    /// Biometric authentication.
    #[display("biometrics")]
    Biometrics,
    /// Device motion and orientation sensor access.
    #[display("motion")]
    Motion,
}

/// One remote-operation permission requested by the product (RFC 0002).
///
/// `ChainSubmit`, `PreimageSubmit`, and `StatementSubmit` are also triggered
/// implicitly by the corresponding business calls when not yet granted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Enum)
)]
pub enum RemotePermission {
    /// Outbound HTTP/WebSocket access to a set of domains. External navigation
    /// uses [`HostDevicePermissionRequest::OpenUrl`] instead.
    #[display("access to {}", domains.join(", "))]
    Remote {
        /// Domain patterns requested by the product. Each is an exact host, a
        /// wildcard covering every descendant (`*.example.com`), or `*` for any host.
        /// Wildcard suffixes must be domain names with at least two labels.
        domains: Vec<String>,
    },
    /// WebRTC access.
    ///
    /// The container authorizes each peer connection through Rust before its
    /// first network method. Later methods on that connection share the same
    /// decision, so a one-use grant permits one connection. New connections
    /// check current permissions without requiring a page reload.
    ///
    /// Camera and microphone capture is gated by the OS permission prompts and
    /// [`HostDevicePermissionRequest`], not by this permission.
    #[display("WebRTC connections")]
    WebRtc,
    /// Submitting transactions on behalf of the user via `remote_chain_transaction_broadcast`.
    #[display("submit chain transactions")]
    ChainSubmit,
    /// Submitting preimages on behalf of the user via `remote_preimage_submit`.
    #[display("submit preimages")]
    PreimageSubmit,
    /// Submitting statements on behalf of the user via `remote_statement_store_submit`.
    #[display("submit statements")]
    StatementSubmit,
}

/// remote-permission request (RFC 0002).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, Display)]
#[display("{permission}")]
#[cfg_attr(
    all(feature = "runtime", not(target_arch = "wasm32")),
    derive(uniffi::Record)
)]
pub struct RemotePermissionRequest {
    /// Permission requested by the product.
    pub permission: RemotePermission,
}

/// Outcome of a device-permission request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostDevicePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}

/// Outcome of a remote-permission request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct RemotePermissionResponse {
    /// Whether the permission was granted.
    pub granted: bool,
}
