//! dotNS protocol-registry view helpers shared by the gateway and the
//! runtime's manifest readers.

use alloy_primitives::B256;
use alloy_sol_types::SolCall;

use crate::host_logic::dotns_gateway::{
	DotnsCallError, DotnsError, DotnsRegistry, DotnsTransport, DotnsViewError, ProtocolRegistry,
	namehash_under,
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
	let output = transport.view(dest, call.abi_encode()).await?;
	Ok(Call::abi_decode_returns_validate(&output)?)
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
	match read_view(transport, registry, ProtocolRegistry::tldCall {}).await {
		Ok(tld) => return Ok(tld),
		Err(DotnsCallError::View(DotnsViewError::Reverted { .. })) => {},
		Err(error) => return Err(DotnsError::Tld(error)),
	}
	let dotns_registry = protocol_component(transport, registry, "registry").await?;
	let exists = read_view(
		transport,
		&dotns_registry,
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
