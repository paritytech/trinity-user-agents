//! The products published to browse, the on-chain app directory.
//!
//! A browse `Publisher` contract keeps the label hashes of the products
//! published to it. Its deployments are not registered with dotNS, so they are
//! compiled in per network, keyed by the Asset Hub genesis, as browse-sdk does.
//! The registrar turns each hash back into a label. The list is cached like
//! the manifests, so a published product appears, and an unpublished one drops
//! out, within the cache lifetime.

use serde_json::json;
use sp_crypto_hashing::keccak_256;
use tracing::instrument;

use super::{
	dotns_lookup::DotnsLookup,
	product_manifest::{cached_dotns_json, protocol_registry},
	services::RuntimeServices,
};
use crate::{
	chain_runtime::ChainRuntime,
	dotns_views::{network_tld, protocol_component, tld_node},
	host_logic::dotns_gateway::{
		DotnsTransport, call_bytes32, call_u256_pair, decode_bytes32_array, decode_string,
	},
	platform::{CoreStorageKey, Platform},
};

/// Every `Publisher` deployment on each network, keyed by the Asset Hub
/// genesis and newest first, as browse-sdk lists them.
const PUBLISHERS: &[([u8; 32], &[[u8; 20]])] =
	&[(PASEO_NEXT_V2, &[PUBLISHER_3_1_0, PUBLISHER_3_0_0])];

/// Asset Hub genesis of Paseo Next v2.
const PASEO_NEXT_V2: [u8; 32] = [
	0x43, 0x49, 0xb0, 0x0e, 0x54, 0x89, 0x7e, 0x21, 0x19, 0x6f, 0xd3, 0x31, 0x01, 0x5f, 0xc5, 0xbe,
	0x0f, 0x14, 0xe1, 0x18, 0xbe, 0xb0, 0x37, 0x5e, 0xd2, 0xbb, 0x17, 0x93, 0x73, 0x7b, 0xb5, 0x7a,
];

/// The 3.1.0 `Publisher` on Paseo Next v2.
const PUBLISHER_3_1_0: [u8; 20] = [
	0xaa, 0x18, 0x9b, 0x1d, 0x4f, 0x65, 0xcf, 0x6e, 0x5d, 0x0b, 0xad, 0x73, 0x4f, 0x6c, 0x87, 0x6f,
	0x2e, 0x12, 0xf9, 0x84,
];

/// The 3.0.0 `Publisher` on Paseo Next v2.
const PUBLISHER_3_0_0: [u8; 20] = [
	0x01, 0x16, 0x7f, 0x22, 0x8a, 0x72, 0x9f, 0x8e, 0x50, 0xf1, 0x8a, 0xa7, 0x18, 0x9f, 0x59, 0xb6,
	0x59, 0x15, 0x5d, 0x09,
];

/// Label hashes read per `getPublished` call.
const PAGE_LIMIT: u64 = 1000;

/// The product ids published to browse on the host's network, in the order
/// the Publishers list them. Empty on a network with no Publisher.
pub async fn published_products(
	services: &RuntimeServices,
	platform: &dyn Platform,
) -> Result<Vec<String>, String> {
	let Some(genesis_hash) = services.asset_hub_chain_genesis_hash() else {
		return Ok(Vec::new());
	};
	let Some(publishers) = publishers_on(genesis_hash) else {
		return Ok(Vec::new());
	};
	let json = cached_dotns_json(platform, CoreStorageKey::PublishedProducts, || {
		fetch_published_products(&services.chain, genesis_hash, publishers)
	})
	.await?;
	Ok(json.and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default())
}

/// The Publisher addresses on the network whose Asset Hub has `genesis_hash`.
fn publishers_on(genesis_hash: [u8; 32]) -> Option<&'static [[u8; 20]]> {
	PUBLISHERS
		.iter()
		.find(|(network, _)| *network == genesis_hash)
		.map(|(_, publishers)| *publishers)
}

/// Every label published to `publishers`, as product ids under the network's
/// TLD, as a JSON array. A label hash two Publishers both list appears once,
/// and one the registrar cannot name is skipped.
#[instrument(skip_all, fields(runtime.method = "browse.published_products"))]
async fn fetch_published_products(
	chain: &ChainRuntime,
	genesis_hash: [u8; 32],
	publishers: &[[u8; 20]],
) -> Result<Option<String>, String> {
	let mut lookup = DotnsLookup::pinned_to_best_block(chain, genesis_hash, "browse:published")
		.await
		.map_err(|error| error.to_string())?;
	let Some(protocol_registry) =
		protocol_registry(&mut lookup).await.map_err(|error| error.reason())?
	else {
		return Ok(None);
	};
	let tld = network_tld(&mut lookup, &protocol_registry)
		.await
		.map_err(|error| error.reason())?;
	let registrar = protocol_component(&mut lookup, &protocol_registry, "registrar")
		.await
		.map_err(|error| error.reason())?;

	let mut label_hashes: Vec<[u8; 32]> = Vec::new();
	for publisher in publishers {
		for offset in (0..).step_by(PAGE_LIMIT as usize) {
			let output = lookup
				.view(
					publisher,
					call_u256_pair("getPublished(uint256,uint256)", offset, PAGE_LIMIT),
				)
				.await
				.map_err(|err| format!("Publisher.getPublished(): {err}"))?;
			let page = decode_bytes32_array(&output)
				.map_err(|err| format!("Publisher.getPublished(): {err}"))?;
			let full = page.len() as u64 == PAGE_LIMIT;
			for label_hash in page {
				if !label_hashes.contains(&label_hash) {
					label_hashes.push(label_hash);
				}
			}
			if !full {
				break;
			}
		}
	}

	let tld_node = tld_node(&tld);
	let mut products = Vec::with_capacity(label_hashes.len());
	for label_hash in label_hashes {
		// The registrar's token id for a label is the label's node.
		let node = keccak_256(&[tld_node.as_slice(), label_hash.as_slice()].concat());
		let label = match lookup.view(&registrar, call_bytes32("labelOf(uint256)", &node)).await {
			Ok(output) => decode_string(&output).unwrap_or_default(),
			Err(err) => {
				tracing::warn!(%err, "Registrar.labelOf() failed for a published label");
				continue;
			},
		};
		if !label.is_empty() && !label.contains('.') {
			products.push(format!("{label}{tld}"));
		}
	}
	Ok(Some(json!(products).to_string()))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn paseo_next_v2_lists_both_publisher_deployments() {
		assert_eq!(
			(publishers_on(PASEO_NEXT_V2).map(<[[u8; 20]]>::len), publishers_on([0; 32])),
			(Some(2), None)
		);
	}
}
