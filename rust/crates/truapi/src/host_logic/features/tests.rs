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
	let resp =
		futures::executor::block_on(feature_supported(&AlwaysSupported, |_, _| false, req()))
			.unwrap();
	assert!(resp.supported);
}

#[test]
fn delegates_unsupported_to_platform() {
	let resp =
		futures::executor::block_on(feature_supported(&AlwaysUnsupported, |_, _| false, req()))
			.unwrap();
	assert!(!resp.supported);
}

/// The materials question is the core's to answer, from what it reads and
/// whether the host draws materials: the platform is not asked, whatever it
/// would say.
#[test]
fn the_core_answers_the_materials_question_not_the_platform() {
	let request = || HostFeatureSupportedRequest::Materials {
		format_version: 2,
		features: vec!["sl-pressure".to_string()],
	};
	let answer = |platform: &dyn Features, draws: bool| {
		let mut asked = None;
		let response = futures::executor::block_on(feature_supported(
			platform,
			|version, features: &[String]| {
				asked = Some((version, features.to_vec()));
				draws
			},
			request(),
		))
		.unwrap();
		(response.supported, asked)
	};

	let asked = Some((2, vec!["sl-pressure".to_string()]));
	assert_eq!(answer(&AlwaysSupported, false), (false, asked.clone()));
	assert_eq!(answer(&AlwaysUnsupported, true), (true, asked));
}
