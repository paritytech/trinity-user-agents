//! dotNS protocol-registry view helpers shared by the gateway and the
//! runtime's manifest readers.

use crate::host_logic::dotns_gateway::{
	DotnsCallError, DotnsContractError, DotnsError, DotnsTransport, DotnsViewError, call_bytes32,
	call_no_args, decode_address, decode_bool, decode_string, namehash_under, registry_key,
};

/// The TLD of networks whose `DotnsProtocolRegistry` has no `tld()` view;
/// previewnet is one.
pub const TLD_WITHOUT_VIEW: &str = ".dot";

/// Appends a dynamic `string` tail: its length, then its bytes padded to a
/// whole number of words. The caller has already written the head offset.
fn append_dynamic_string(data: &mut Vec<u8>, value: &str) {
	let bytes = value.as_bytes();
	let mut len = [0u8; 32];
	len[24..].copy_from_slice(&(bytes.len() as u64).to_be_bytes());
	data.extend_from_slice(&len);
	data.extend_from_slice(bytes);
	data.extend(std::iter::repeat_n(0u8, bytes.len().div_ceil(32) * 32 - bytes.len()));
}

/// Head word holding the byte offset a dynamic argument's tail starts at,
/// counted from the end of the selector. `head_words` is how many words the
/// head occupies.
fn dynamic_offset(head_words: usize) -> [u8; 32] {
	let mut word = [0u8; 32];
	word[24..].copy_from_slice(&((head_words * 32) as u64).to_be_bytes());
	word
}

/// Calldata for a view function taking one `string` argument.
pub fn call_string(signature: &str, value: &str) -> Vec<u8> {
	let mut data = call_no_args(signature);
	data.extend_from_slice(&dynamic_offset(1));
	append_dynamic_string(&mut data, value);
	data
}

/// Calldata for a view function taking a `bytes32` and a `string`, such as
/// `text(bytes32 node, string key)`. The string is dynamic, so the head holds
/// its offset and the tail follows.
pub fn call_bytes32_string(signature: &str, word: &[u8; 32], value: &str) -> Vec<u8> {
	let mut data = call_no_args(signature);
	data.extend_from_slice(word);
	data.extend_from_slice(&dynamic_offset(2));
	append_dynamic_string(&mut data, value);
	data
}

/// Runs a contract view and decodes its return data, retaining which stage failed.
pub async fn read_view<T: DotnsTransport + ?Sized, Output>(
	transport: &mut T,
	dest: &[u8; 20],
	input: Vec<u8>,
	decode: fn(&[u8]) -> Result<Output, DotnsContractError>,
) -> Result<Output, DotnsCallError> {
	let output = transport.view(dest, input).await?;
	Ok(decode(&output)?)
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
		call_bytes32("get(bytes32)", &registry_key(name)),
		decode_address,
	)
	.await
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
	match read_view(transport, registry, call_no_args("tld()"), decode_string).await {
		Ok(tld) => return Ok(tld),
		Err(DotnsCallError::View(DotnsViewError::Reverted(_))) => {},
		Err(error) => return Err(DotnsError::Tld(error)),
	}
	let dotns_registry = protocol_component(transport, registry, "registry").await?;
	let exists = read_view(
		transport,
		&dotns_registry,
		call_bytes32("recordExists(bytes32)", &tld_node(TLD_WITHOUT_VIEW)),
		decode_bool,
	)
	.await
	.map_err(DotnsError::RecordExists)?;
	if exists { Ok(TLD_WITHOUT_VIEW.to_string()) } else { Err(DotnsError::MissingTld) }
}

/// The node of the network TLD: `namehash(tld)` for a single-label TLD.
pub fn tld_node(tld: &str) -> [u8; 32] {
	namehash_under(&[0u8; 32], tld.trim_start_matches('.'))
}
