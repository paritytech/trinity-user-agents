use super::*;

struct AlwaysSupported;

#[async_trait::async_trait]
impl Features for AlwaysSupported {
	async fn feature_supported(
		&self,
		request: HostFeatureSupportedRequest,
	) -> Result<HostFeatureSupportedResponse, GenericError> {
		assert!(matches!(request, HostFeatureSupportedRequest::Chain { .. }));
		Ok(HostFeatureSupportedResponse { supported: true })
	}
}

struct AlwaysUnsupported;

#[async_trait::async_trait]
impl Features for AlwaysUnsupported {
	async fn feature_supported(
		&self,
		request: HostFeatureSupportedRequest,
	) -> Result<HostFeatureSupportedResponse, GenericError> {
		assert!(matches!(request, HostFeatureSupportedRequest::Chain { .. }));
		Ok(HostFeatureSupportedResponse { supported: false })
	}
}

fn req() -> HostFeatureSupportedRequest {
	HostFeatureSupportedRequest::Chain { genesis_hash: vec![0u8; 32] }
}

#[test]
fn delegates_supported_to_platform() {
	let resp = futures::executor::block_on(feature_supported(&AlwaysSupported, req())).unwrap();
	assert!(resp.supported);
}

#[test]
fn delegates_unsupported_to_platform() {
	let resp = futures::executor::block_on(feature_supported(&AlwaysUnsupported, req())).unwrap();
	assert!(!resp.supported);
}

/// No host draws materials yet, so a platform that says yes to anything is
/// not asked, and a product never sends a tree the host would draw plain.
#[test]
fn materials_are_not_supported_whatever_the_platform_says() {
	let request =
		HostFeatureSupportedRequest::Materials { format_version: 1, features: Vec::new() };
	let resp = futures::executor::block_on(feature_supported(&AlwaysSupported, request)).unwrap();
	assert!(!resp.supported);
}
