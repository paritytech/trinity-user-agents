use crate::{
	Error,
	format::{self, Schemes},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
// `unwrap_or_skip` reads a field that may be left out but, when present, holds a value: `null`
// is not absent.
use serde_with::{rust::unwrap_or_skip, skip_serializing_none};
use std::collections::{BTreeMap, HashSet};

/// The only format version this crate reads and writes.
pub const VERSION: u64 = 1;

/// The service kinds `$v: 1` defines. Readers drop entries of any other kind before validating.
pub const SERVICE_KINDS: &[&str] =
	&["dub", "turn", "stun", "ipfs-gateway", "hop", "eth-rpc", "storage-provider", "faucet"];

/// A validated `$v: 1` descriptor, with unknown fields and service kinds already dropped.
///
/// Deserialising one applies the same rules as [`Descriptor::from_json`], so no path skips them.
/// The I-JSON checks need the bytes as served: read those with [`Document::parse`].
///
/// [`Document::parse`]: crate::Document::parse
// A plain derive would generate the `Deserialize` impl itself, skipping validation. With
// `remote = "Self"` the derives become inherent functions, and the trait impls below call them,
// adding `from_json`'s checks on the way in.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[serde(remote = "Self", rename_all = "camelCase")]
pub struct Descriptor {
	/// Format version; always [`VERSION`].
	#[serde(rename = "$v")]
	#[cfg_attr(feature = "js", tsify(type = "1"))]
	pub version: u64,
	/// Raised on every publish.
	pub sequence: u32,
	/// Opaque; changes whenever the stack restarts its chains.
	pub generation: String,
	/// The stack's name. Never decides trust and is not unique.
	pub id: String,
	/// Display name.
	pub name: String,
	/// The ecosystem `get_chain_info` reports to dapps.
	pub network: String,
	/// dotNS TLD without the leading dot.
	pub tld: String,
	/// When the stack itself shuts down, in Unix seconds.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub expires_at: Option<u32>,
	/// When this copy goes stale and must be fetched again, in Unix seconds.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub valid_until: Option<u32>,
	/// Set when the stack is a fork of another. Explains; never decides trust.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub fork: Option<Fork>,
	/// The stack's chains.
	pub chains: Vec<Chain>,
	/// The stack's off-chain services of known kinds.
	pub services: Vec<Service>,
	/// Assets readers treat specially.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub assets: Option<Vec<Asset>>,
	/// Asset Hub contracts readers need that the chains do not point to, by name.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub contracts: Option<BTreeMap<String, String>>,
	/// Chain role and instance of the Coinage pallet.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub coinage: Option<Coinage>,
}

/// One chain of a stack.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Chain {
	/// RFC 0026 role, such as `Relay` or `AssetHub`, or another role that serves tools.
	pub role: String,
	/// 0x-prefixed genesis hash.
	pub genesis_hash: String,
	/// Required on every chain but the relay.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub para_id: Option<u32>,
	/// WebSocket endpoints, most preferred first.
	pub rpc: Vec<String>,
	/// The chain's nodes serve the statement store.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub statement_store: Option<bool>,
	/// Light-client chain spec.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub light_spec: Option<SpecFile>,
	/// Full chain spec with the whole genesis storage.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub full_spec: Option<SpecFile>,
	/// The source's block this chain was forked at.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub forked_at: Option<Block>,
}

/// A chain spec file and the digest of its bytes as served.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct SpecFile {
	/// Where the spec is served.
	pub url: String,
	/// SHA-256 of the bytes as served, before HTTP compression.
	pub sha256: String,
}

/// A block by number and hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct Block {
	/// Block number.
	pub number: u32,
	/// 0x-prefixed block hash.
	pub hash: String,
}

/// An off-chain service.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct Service {
	/// One of [`SERVICE_KINDS`].
	pub kind: String,
	/// Endpoints, most preferred first.
	pub urls: Vec<String>,
	/// The EVM chain id; set on `eth-rpc` only. Up to 2^53 - 1, since EVM chain ids can exceed
	/// 2^32 and every JSON reader holds that range exactly.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub chain_id: Option<u64>,
}

/// The largest `chainId`: every JSON reader holds integers up to 2^53 - 1 exactly.
const MAX_CHAIN_ID: u64 = (1 << 53) - 1;

/// An asset readers treat specially.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct Asset {
	/// What it is for, such as `payment`.
	pub role: String,
	/// Role of the chain that holds it.
	pub chain: String,
	/// Pallet that holds it.
	pub pallet: String,
	/// Its id in that pallet.
	pub id: u32,
}

/// Where the Coinage pallet runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct Coinage {
	/// Role of the chain that runs it.
	pub chain: String,
	/// Its instance.
	pub instance: u32,
}

/// The stack a fork was made from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct Fork {
	/// The source stack's pointer.
	pub of: ForkSource,
	/// When it was forked, in Unix seconds.
	pub at: u32,
}

/// The pointer of a fork's source stack.
#[skip_serializing_none]
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct ForkSource {
	/// The source descriptor's URL.
	pub url: String,
	/// The source's public key, when it is signed.
	#[serde(default, deserialize_with = "unwrap_or_skip::deserialize")]
	pub key: Option<String>,
}

impl Descriptor {
	/// Reads a descriptor from a parsed I-JSON document: drops service entries of unknown kinds,
	/// then checks every format rule on what remains.
	pub fn from_json(mut document: serde_json::Value) -> Result<Self, Error> {
		let version = version_of(&document)?;
		if version != VERSION {
			return Err(Error::UnsupportedVersion(version));
		}
		if let Some(services) = document.get_mut("services").and_then(|value| value.as_array_mut())
		{
			services.retain(|service| {
				service.get("kind").and_then(|kind| kind.as_str()).is_none_or(is_known_kind)
			});
		}
		let descriptor =
			Self::deserialize(document).map_err(|error| Error::Malformed(error.to_string()))?;
		descriptor.validate()?;
		Ok(descriptor)
	}

	/// The chain with this role.
	pub fn chain(&self, role: &str) -> Option<&Chain> {
		self.chains.iter().find(|chain| chain.role == role)
	}

	/// Checks the RFC's format rules on known fields.
	fn validate(&self) -> Result<(), Error> {
		if self.generation.is_empty() {
			return Err(Error::Empty("generation"));
		}
		if !is_label(&self.tld) {
			return Err(Error::InvalidTld(self.tld.clone()));
		}
		self.validate_chains()?;
		self.validate_services()?;
		self.validate_extras()?;
		self.validate_fork()
	}

	fn validate_chains(&self) -> Result<(), Error> {
		let mut roles = HashSet::new();
		let mut para_ids = HashSet::new();
		for chain in &self.chains {
			if !roles.insert(chain.role.as_str()) {
				return Err(Error::DuplicateChainRole(chain.role.clone()));
			}
			match (chain.role == "Relay", chain.para_id) {
				(true, Some(_)) => return Err(Error::ParaId("set on the relay".into())),
				(false, None) => return Err(Error::ParaId(format!("missing on {}", chain.role))),
				(false, Some(para_id)) if !para_ids.insert(para_id) => {
					return Err(Error::ParaId(format!("{para_id} appears more than once")));
				},
				_ => {},
			}
			format::hex_bytes::<32>("genesisHash", &chain.genesis_hash)?;
			if chain.rpc.is_empty() {
				return Err(Error::Empty("rpc"));
			}
			for rpc in &chain.rpc {
				format::url(rpc, Schemes::Web)?;
			}
			for spec in [&chain.light_spec, &chain.full_spec].into_iter().flatten() {
				format::url(&spec.url, Schemes::Web)?;
				format::hex_bytes::<32>("sha256", &spec.sha256)?;
			}
			if let Some(block) = &chain.forked_at {
				format::hex_bytes::<32>("forkedAt.hash", &block.hash)?;
			}
		}
		let relays = self.chains.iter().filter(|chain| chain.role == "Relay").count();
		if relays != 1 {
			return Err(Error::RelayCount(relays));
		}
		Ok(())
	}

	fn validate_services(&self) -> Result<(), Error> {
		let mut kinds = HashSet::new();
		for service in &self.services {
			if !kinds.insert(service.kind.as_str()) {
				return Err(Error::DuplicateServiceKind(service.kind.clone()));
			}
			if (service.kind == "eth-rpc") != service.chain_id.is_some() ||
				service.chain_id.is_some_and(|id| id > MAX_CHAIN_ID)
			{
				return Err(Error::ChainId);
			}
			if service.urls.is_empty() {
				return Err(Error::Empty("urls"));
			}
			let schemes = if service.kind == "stun" { Schemes::Stun } else { Schemes::Web };
			for url in &service.urls {
				let parsed = format::url(url, schemes)?;
				let is_http = matches!(parsed.scheme(), "http" | "https");
				let is_base =
					url.ends_with('/') && parsed.query().is_none() && parsed.fragment().is_none();
				if is_http && service.kind != "eth-rpc" && !is_base {
					return Err(Error::ServiceUrlNotBase(url.clone()));
				}
			}
		}
		Ok(())
	}

	fn validate_extras(&self) -> Result<(), Error> {
		let mut asset_roles = HashSet::new();
		for asset in self.assets.iter().flatten() {
			if !asset_roles.insert(asset.role.as_str()) {
				return Err(Error::DuplicateAssetRole(asset.role.clone()));
			}
			self.require_chain(&asset.chain)?;
		}
		if let Some(coinage) = &self.coinage {
			self.require_chain(&coinage.chain)?;
		}
		for address in self.contracts.iter().flat_map(|contracts| contracts.values()) {
			format::hex_bytes::<20>("contracts", address)?;
		}
		Ok(())
	}

	fn validate_fork(&self) -> Result<(), Error> {
		let is_fork = self.fork.is_some();
		if self.chains.iter().any(|chain| chain.forked_at.is_some() != is_fork) {
			return Err(Error::ForkedAt);
		}
		if let Some(fork) = &self.fork {
			format::url(&fork.of.url, Schemes::Web)?;
			if let Some(key) = &fork.of.key {
				format::hex_bytes::<32>("fork.of.key", key)?;
			}
		}
		Ok(())
	}

	fn require_chain(&self, role: &str) -> Result<(), Error> {
		match self.chain(role) {
			Some(_) => Ok(()),
			None => Err(Error::UnknownChainRole(role.to_owned())),
		}
	}
}

impl Serialize for Descriptor {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		Descriptor::serialize(self, serializer)
	}
}

impl<'de> Deserialize<'de> for Descriptor {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let document = serde_json::Value::deserialize(deserializer)?;
		Self::from_json(document)
			.map_err(|error| de::Error::custom(format!("{}: {error}", error.code())))
	}
}

/// The `$v` member of a document.
pub fn version_of(document: &serde_json::Value) -> Result<u64, Error> {
	document
		.get("$v")
		.and_then(|version| version.as_u64())
		.ok_or_else(|| Error::Malformed("$v must be a non-negative integer".into()))
}

/// One lowercase DNS label, so a TLD compares by its bytes and cannot pass as another (`.dot`,
/// `DOT`).
fn is_label(name: &str) -> bool {
	let allowed = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
	(1..=63).contains(&name.len()) &&
		name.bytes().all(allowed) &&
		!name.starts_with('-') &&
		!name.ends_with('-')
}

fn is_known_kind(kind: &str) -> bool {
	SERVICE_KINDS.contains(&kind)
}

#[cfg(test)]
mod tests {
	use super::*;

	const RELAY: &str = r#"[{"role":"Relay","genesisHash":"0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92","rpc":["ws://127.0.0.1:9944"]}]"#;

	#[test]
	fn deserialising_directly_applies_the_same_rules() {
		let broken = format!(
			r#"{{"$v":1,"sequence":1,"generation":"g","id":"a","name":"A","network":"n","tld":".dot","services":[],"chains":{RELAY}}}"#
		);
		let error = serde_json::from_str::<Descriptor>(&broken).unwrap_err();
		assert!(error.to_string().starts_with("invalid-tld:"), "{error}");
		let valid: Descriptor = serde_json::from_str(&broken.replace(".dot", "dot")).unwrap();
		let round_trip: Descriptor =
			serde_json::from_str(&serde_json::to_string(&valid).unwrap()).unwrap();
		assert_eq!(round_trip, valid);
	}
}
