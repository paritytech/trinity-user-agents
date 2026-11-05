//! Reading a stored blob back by its CID with a Bulletin node's `bitswap_v1_get`.

use serde_json::Value;
use subxt_rpcs::{
	UserError,
	client::{RpcClient, rpc_params},
};

use super::{BASE32_ALPHABET, PREIMAGE_CID_PREFIX, preimage_cid};

// `bitswap_v1_get` errors by code, the only part of the error the bitswap JSON-RPC spec
// (`bitswap_unstable_get`) makes stable.
/// The data is not held. It can still arrive.
const FAIL: i32 = -32810;
/// The CID is invalid or unsupported, and must not be sent again.
const INVALID_PARAMS: i32 = -32602;
/// The node has no `bitswap_v1_get`.
const METHOD_NOT_FOUND: i32 = -32601;

/// The preimage key a [`preimage_cid`] names. `None` for any other CID.
pub fn preimage_key_from_cid(cid: &str) -> Option<[u8; 32]> {
	let mut bytes = Vec::new();
	let (mut buffer, mut bits) = (0u32, 0u32);
	for symbol in cid.strip_prefix('b')?.bytes() {
		let value = BASE32_ALPHABET.iter().position(|&candidate| candidate == symbol)?;
		buffer = (buffer << 5) | value as u32;
		bits += 5;
		if bits >= 8 {
			bits -= 8;
			bytes.push((buffer >> bits) as u8);
			buffer &= (1 << bits) - 1;
		}
	}
	let key: [u8; 32] = bytes.strip_prefix(&PREIMAGE_CID_PREFIX)?.try_into().ok()?;
	(preimage_cid(&key) == cid).then_some(key)
}

/// Why `bitswap_v1_get` returned no blob.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display)]
pub enum BitswapError {
	/// The node can never answer this request, such as one for an invalid CID.
	Unanswerable(String),
	/// The node serves no `bitswap_v1_get`. Another node behind the same endpoint may.
	MethodMissing(String),
	/// The connection failed, so a new one may succeed.
	Connection(String),
	/// The node failed the request, and asking again may succeed.
	Failed(String),
}

/// The blob the node behind `client` holds under `cid`, or `None` while it holds none. The
/// bytes are unchecked: the caller compares them with the CID.
pub async fn bitswap_get(client: &RpcClient, cid: &str) -> Result<Option<Vec<u8>>, BitswapError> {
	let value = match client.request::<Value>("bitswap_v1_get", rpc_params![cid]).await {
		Ok(value) => value,
		Err(subxt_rpcs::Error::User(error)) => return call_error(&error),
		Err(error) => return Err(BitswapError::Connection(format!("bitswap_v1_get: {error}"))),
	};
	let hex = value.as_str().and_then(|value| value.strip_prefix("0x"));
	match hex.map(hex::decode) {
		Some(Ok(bytes)) => Ok(Some(bytes)),
		_ => Err(BitswapError::Failed(format!(
			"bitswap_v1_get: expected 0x-prefixed hex, got {value}"
		))),
	}
}

/// A `bitswap_v1_get` call error, by code. A CID the node does not hold is answered with an
/// error, not with null.
fn call_error(error: &UserError) -> Result<Option<Vec<u8>>, BitswapError> {
	let reason = format!("bitswap_v1_get: {} ({})", error.message, error.code);
	match error.code {
		FAIL => Ok(None),
		INVALID_PARAMS => Err(BitswapError::Unanswerable(reason)),
		METHOD_NOT_FOUND => Err(BitswapError::MethodMissing(reason)),
		_ => Err(BitswapError::Failed(reason)),
	}
}

#[cfg(test)]
mod tests;
