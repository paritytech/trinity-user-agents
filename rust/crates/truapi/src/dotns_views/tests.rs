use super::*;

struct ExistingRegistry;

#[async_trait::async_trait]
impl DotnsTransport for ExistingRegistry {
	async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
		panic!("the deployment was already resolved")
	}

	async fn view(&mut self, dest: &[u8; 20], input: Vec<u8>) -> Result<Vec<u8>, EvmContractError> {
		let call = DotnsRegistry::recordExistsCall::abi_decode(&input).unwrap();
		assert_eq!((*dest, call.node.0), ([0x33; 20], tld_node(TLD_WITHOUT_VIEW)));
		Ok(DotnsRegistry::recordExistsCall::abi_encode_returns(&true))
	}

	async fn views(
		&mut self,
		_dest: &[u8; 20],
		_inputs: Vec<Vec<u8>>,
	) -> Vec<Result<Vec<u8>, EvmContractError>> {
		unreachable!("this lookup uses individual views")
	}
}

#[test]
fn a_batched_tld_fallback_reuses_the_registry_already_read() {
	let output = Err(EvmContractError::Reverted { detail: "tld view absent".into() });

	assert_eq!(
		futures::executor::block_on(network_tld_from_view(
			&mut ExistingRegistry,
			output,
			&[0x33; 20],
		))
		.unwrap(),
		TLD_WITHOUT_VIEW
	);
}

#[test]
fn a_failed_batched_tld_does_not_use_the_fallback() {
	for output in [
		Err(EvmContractError::Simulation(Box::new(subxt::Error::other_str("offline")))),
		Err(EvmContractError::Dispatch { detail: "0x00".into() }),
		Err(EvmContractError::Malformed("invalid SCALE".into())),
		Ok(vec![0xff]),
	] {
		let result = futures::executor::block_on(network_tld_from_view(
			&mut ExistingRegistry,
			output,
			&[0x33; 20],
		));

		assert!(matches!(result, Err(DotnsError::Tld(_))), "{result:?}");
	}
}
