use super::*;
use crate::platform::HostChainEntry;
use truapi::latest::ChainIdentifier;

fn paseo_set() -> HostChainSet {
	HostChainSet {
		network: "paseo".to_string(),
		chains: vec![
			HostChainEntry { identifier: ChainIdentifier::AssetHub, genesis_hash: [0xaa; 32] },
			HostChainEntry { identifier: ChainIdentifier::People, genesis_hash: [0xbb; 32] },
		],
	}
}

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

	async fn supported_chains(&self) -> Result<HostChainSet, GenericError> {
		Ok(paseo_set())
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

	async fn supported_chains(&self) -> Result<HostChainSet, GenericError> {
		Err(GenericError { reason: "no chains".to_string() })
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

#[test]
fn delegates_supported_chains_to_platform() {
	let set = futures::executor::block_on(supported_chains(&AlwaysSupported)).unwrap();
	assert_eq!(set, paseo_set());
}

#[test]
fn surfaces_supported_chains_platform_error() {
	let err = futures::executor::block_on(supported_chains(&AlwaysUnsupported)).unwrap_err();
	assert_eq!(err.reason, "no chains");
}

#[test]
fn resolves_identifier_and_echoes_it() {
	let request = RemoteChainInfoRequest { chain: ChainIdentifier::People };
	let response = chain_info(&paseo_set(), &request).unwrap();
	assert_eq!(response.network, "paseo");
	assert_eq!(response.chain, ChainIdentifier::People);
	assert_eq!(response.genesis_hash, [0xbb; 32]);
}

#[test]
fn unserved_identifier_is_not_supported() {
	let request = RemoteChainInfoRequest { chain: ChainIdentifier::Bulletin };
	let err = chain_info(&paseo_set(), &request).unwrap_err();
	assert_eq!(err, RemoteChainInfoError::NotSupported);
}
