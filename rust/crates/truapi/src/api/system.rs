//! Unified [`System`] trait.

use crate::{
	CallContext, CallError,
	versioned::system::{
		HostFeatureSupportedError, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
		HostGetProductContextError, HostGetProductContextRequest, HostGetProductContextResponse,
		HostHandshakeError, HostHandshakeRequest, HostHandshakeResponse, HostInfoError,
		HostInfoRequest, HostInfoResponse, HostNavigateToError, HostNavigateToRequest,
		HostNavigateToResponse,
	},
	wire, wire_trait,
};

/// General-purpose TrUAPI methods for handshake, feature detection,
/// navigation, and runtime information.
#[wire_trait(id = 1)]
#[crate::async_trait]
pub trait System: Send + Sync {
	/// Negotiate the wire codec version with the product.
	///
	/// ```ts
	/// const result = await truapi.system.handshake();
	/// assert(result.isOk(), "handshake failed:", result);
	/// console.log("handshake succeeded");
	/// ```
	#[wire(id = 0)]
	async fn handshake(
		&self,
		_cx: &CallContext,
		request: HostHandshakeRequest,
	) -> Result<HostHandshakeResponse, CallError<HostHandshakeError>> {
		let HostHandshakeRequest::V1(version) = request;
		if version.codec_version == crate::WIRE_CODEC_VERSION {
			Ok(HostHandshakeResponse::V1)
		} else {
			Err(CallError::Domain(HostHandshakeError::V1(
				crate::v01::HostHandshakeError::UnsupportedProtocolVersion,
			)))
		}
	}

	/// Query whether the host supports a specific feature.
	///
	/// ```ts
	/// const assetHub = await truapi.chain.getChainInfo({ chain: "AssetHub" });
	/// assert(assetHub.isOk(), "getChainInfo failed:", assetHub);
	///
	/// const result = await truapi.system.featureSupported({
	///   tag: "Chain",
	///   value: {
	///     genesisHash: assetHub.value.genesisHash,
	///   },
	/// });
	/// assert(result.isOk(), "featureSupported failed:", result);
	/// console.log("feature supported:", result.value.supported);
	/// ```
	#[wire(id = 1)]
	async fn feature_supported(
		&self,
		cx: &CallContext,
		request: HostFeatureSupportedRequest,
	) -> Result<HostFeatureSupportedResponse, CallError<HostFeatureSupportedError>>;

	/// Request the host to open a URL.
	///
	/// An `http` or `https` URL outside the ecosystem needs a
	/// `RemotePermission::Remote` grant for the target host, and prompts for one
	/// on first use. dotNS names, `localhost`, and the app-handoff schemes
	/// (`mailto:`, `tel:`, `polkadot:`, `dot:`) consume no grant. The grant is
	/// per host and shared with outbound data access to that host, so approving
	/// one covers the other.
	///
	/// ```ts
	/// const result = await truapi.system.navigateTo({
	///   url: "https://example.com",
	/// });
	/// assert(result.isOk(), "navigateTo failed:", result);
	/// console.log("navigation succeeded");
	/// ```
	#[wire(id = 2)]
	async fn navigate_to(
		&self,
		cx: &CallContext,
		request: HostNavigateToRequest,
	) -> Result<HostNavigateToResponse, CallError<HostNavigateToError>>;

	/// Report the host's identity and version.
	///
	/// Returns the host's platform, name, and version so a product knows
	/// exactly which host — and which build of it — is running it: for
	/// adapting to the host, telemetry, and attributing behaviour to a
	/// concrete build in diagnostics and bug reports.
	///
	/// ```ts
	/// const result = await truapi.system.info();
	/// assert(result.isOk(), "info failed:", result);
	/// const info = result.value;
	/// console.log(`${info.name} ${info.version} on ${info.platform}`);
	/// ```
	#[wire(id = 3)]
	async fn host_info(
		&self,
		cx: &CallContext,
		request: HostInfoRequest,
	) -> Result<HostInfoResponse, CallError<HostInfoError>>;

	/// Return the product context bound to the current host runtime.
	///
	/// ```ts
	/// const context = await truapi.system.getProductContext();
	/// assert(context.isOk(), "getProductContext failed:", context);
	/// console.log("product id:", context.value.productId);
	/// ```
	#[wire(id = 4)]
	async fn get_product_context(
		&self,
		_cx: &CallContext,
		_request: HostGetProductContextRequest,
	) -> Result<HostGetProductContextResponse, CallError<HostGetProductContextError>> {
		Err(CallError::unavailable())
	}
}
