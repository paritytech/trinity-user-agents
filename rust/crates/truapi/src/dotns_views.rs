//! dotNS protocol-registry view helpers shared by the gateway and the
//! runtime's manifest readers.

use alloy_primitives::B256;
use alloy_sol_types::SolCall;

use crate::{
	chain::EvmContractError,
	host_logic::dotns_gateway::{
		DotnsCallError, DotnsError, DotnsPopController, DotnsRegistry, DotnsTransport,
		ProtocolRegistry, discover_pop_controller, namehash_under,
	},
};

/// The TLD of networks whose `DotnsProtocolRegistry` has no `tld()` view;
/// previewnet is one.
pub const TLD_WITHOUT_VIEW: &str = ".dot";

/// Runs a contract view and decodes its return data, retaining which stage failed.
pub async fn read_view<T: DotnsTransport + ?Sized, Call: SolCall>(
	transport: &mut T,
	dest: &[u8; 20],
	call: Call,
) -> Result<Call::Return, DotnsCallError> {
	decode_view::<Call>(transport.view(dest, call.abi_encode()).await)
}

/// Decodes a completed contract view.
///
/// Retains transport and contract failures as [`DotnsCallError::View`], and
/// invalid ABI return data as [`DotnsCallError::Decode`].
pub fn decode_view<Call: SolCall>(
	output: Result<Vec<u8>, EvmContractError>,
) -> Result<Call::Return, DotnsCallError> {
	Ok(Call::abi_decode_returns_validate(&output?)?)
}

/// Resolves the deployment's protocol registry.
///
/// Returns `None` when no dispatcher is configured. Discovery and contract-view
/// failures return errors.
pub async fn protocol_registry<T: DotnsTransport + ?Sized>(
	transport: &mut T,
) -> Result<Option<[u8; 20]>, DotnsError> {
	let Some(controller) = discover_pop_controller(transport).await? else {
		tracing::warn!("DotnsGateway.DispatcherAddress is unset, so no dotNS records resolve");
		return Ok(None);
	};
	read_view(transport, &controller, DotnsPopController::protocolRegistryCall {})
		.await
		.map(|registry| Some(registry.into_array()))
		.map_err(DotnsError::ProtocolRegistry)
}

/// One component address out of the protocol registry's address book, so a
/// rotated implementation is picked up without a change here.
pub async fn protocol_component<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	protocol_registry: &[u8; 20],
	name: &str,
) -> Result<[u8; 20], DotnsError> {
	read_view(
		transport,
		protocol_registry,
		ProtocolRegistry::getCall { key: B256::right_padding_from(name.as_bytes()) },
	)
	.await
	.map(|address| address.into_array())
	.map_err(|source| DotnsError::ProtocolComponent { name: name.to_string(), source })
}

/// The network TLD with its leading dot (`.paseo`), read from
/// `ProtocolRegistry.tld()`. A registry without that view reverts; the fall
/// back to [`TLD_WITHOUT_VIEW`] is then verified rather than guessed —
/// `DotnsRegistry.recordExists(namehash("dot"))` must hold the TLD's own
/// record, or the resolution errors. Any other failure is an error: a wrong
/// TLD would drop every label carrying the real one.
pub async fn network_tld<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	registry: &[u8; 20],
) -> Result<String, DotnsError> {
	let output = transport.view(registry, ProtocolRegistry::tldCall {}.abi_encode()).await;
	if let Some(tld) = decode_tld(output)? {
		return Ok(tld);
	}
	let dotns_registry = protocol_component(transport, registry, "registry").await?;
	verified_fallback_tld(transport, &dotns_registry).await
}

/// Resolves a completed TLD view against a known registry.
///
/// A reverted view uses `.dot` only when the registry contains that TLD's record.
/// Other view failures and a missing fallback record return errors.
pub async fn network_tld_from_view<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	output: Result<Vec<u8>, EvmContractError>,
	registry: &[u8; 20],
) -> Result<String, DotnsError> {
	if let Some(tld) = decode_tld(output)? {
		return Ok(tld);
	}
	verified_fallback_tld(transport, registry).await
}

fn decode_tld(output: Result<Vec<u8>, EvmContractError>) -> Result<Option<String>, DotnsError> {
	match decode_view::<ProtocolRegistry::tldCall>(output) {
		Ok(tld) => Ok(Some(tld)),
		Err(DotnsCallError::View(EvmContractError::Reverted { .. })) => Ok(None),
		Err(error) => Err(DotnsError::Tld(error)),
	}
}

async fn verified_fallback_tld<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	registry: &[u8; 20],
) -> Result<String, DotnsError> {
	let exists = read_view(
		transport,
		registry,
		DotnsRegistry::recordExistsCall { node: tld_node(TLD_WITHOUT_VIEW).into() },
	)
	.await
	.map_err(DotnsError::RecordExists)?;
	if exists { Ok(TLD_WITHOUT_VIEW.to_string()) } else { Err(DotnsError::MissingTld) }
}

/// The node of the network TLD: `namehash(tld)` for a single-label TLD.
pub fn tld_node(tld: &str) -> [u8; 32] {
	namehash_under(&[0u8; 32], tld.trim_start_matches('.'))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
