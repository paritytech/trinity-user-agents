use crate::{
	Error,
	format::{self, Schemes},
};
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};

/// The only format version this crate reads and writes.
pub const VERSION: u64 = 1;

/// The largest `sequence` a descriptor may carry, 2^53 - 1, so every JSON reader holds it exactly.
pub const MAX_SEQUENCE: u64 = (1 << 53) - 1;

/// The service kinds `$v: 1` defines. Readers drop entries of any other kind before validating.
pub const SERVICE_KINDS: &[&str] =
	&["dub", "turn", "stun", "ipfs-gateway", "hop", "eth-rpc", "storage-provider", "faucet"];

/// A validated `$v: 1` descriptor, with unknown fields and service kinds already dropped.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Descriptor {
	/// Format version; always [`VERSION`].
	#[serde(rename = "$v")]
	pub version: u64,
	/// Raised on every publish.
	pub sequence: u64,
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
	/// When the stack itself shuts down.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub expires_at: Option<String>,
	/// When this copy goes stale and must be fetched again.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub valid_until: Option<String>,
	/// Set when the stack is a fork of another. Explains; never decides trust.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub fork: Option<Fork>,
	/// The stack's chains.
	pub chains: Vec<Chain>,
	/// The stack's off-chain services of known kinds.
	pub services: Vec<Service>,
	/// Assets readers treat specially.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub assets: Option<Vec<Asset>>,
	/// Asset Hub contracts readers need that the chains do not point to, by name.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub contracts: Option<BTreeMap<String, String>>,
	/// Chain role and instance of the Coinage pallet.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub coinage: Option<Coinage>,
}

/// One chain of a stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chain {
	/// RFC 0026 role, such as `Relay` or `AssetHub`, or another role that serves tools.
	pub role: String,
	/// 0x-prefixed genesis hash.
	pub genesis_hash: String,
	/// Required on every chain but the relay.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub para_id: Option<u32>,
	/// WebSocket endpoints, most preferred first.
	pub rpc: Vec<String>,
	/// The chain's nodes serve the statement store.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub statement_store: Option<bool>,
	/// Light-client chain spec.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub light_spec: Option<SpecFile>,
	/// Full chain spec with the whole genesis storage.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub full_spec: Option<SpecFile>,
	/// The source's block this chain was forked at.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub forked_at: Option<Block>,
}

/// A chain spec file and the digest of its bytes as served.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecFile {
	/// Where the spec is served.
	pub url: String,
	/// SHA-256 of the bytes as served, before HTTP compression.
	pub sha256: String,
}

/// A block by number and hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
	/// Block number.
	pub number: u64,
	/// 0x-prefixed block hash.
	pub hash: String,
}

/// An off-chain service.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Service {
	/// One of [`SERVICE_KINDS`].
	pub kind: String,
	/// Endpoints, most preferred first.
	pub urls: Vec<String>,
	/// The EVM chain id; set on `eth-rpc` only.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
	pub chain_id: Option<u64>,
}

/// An asset readers treat specially.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
	/// What it is for, such as `payment`.
	pub role: String,
	/// Role of the chain that holds it.
	pub chain: String,
	/// Pallet that holds it.
	pub pallet: String,
	/// Its id in that pallet.
	pub id: u64,
}

/// Where the Coinage pallet runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coinage {
	/// Role of the chain that runs it.
	pub chain: String,
	/// Its instance.
	pub instance: u64,
}

/// The stack a fork was made from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fork {
	/// The source stack's pointer.
	pub of: ForkSource,
	/// When it was forked.
	pub at: String,
}

/// The pointer of a fork's source stack.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForkSource {
	/// The source descriptor's URL.
	pub url: String,
	/// The source's public key, when it is signed.
	#[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
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
		let descriptor: Self = serde_json::from_value(document)
			.map_err(|error| Error::Malformed(error.to_string()))?;
		descriptor.validate()?;
		Ok(descriptor)
	}

	/// SHA-256 of everything but `sequence`, in this crate's field order. Two publishes compare it
	/// to keep `sequence` while nothing changed; it does not depend on how the source JSON ordered
	/// its keys, so every publisher built on this crate agrees on it.
	pub fn content_hash(&self) -> [u8; 32] {
		let unsequenced = Self { sequence: 0, ..self.clone() };
		let bytes = serde_json::to_vec(&unsequenced).expect("a descriptor always serialises");
		Sha256::digest(bytes).into()
	}

	/// The chain with this role.
	pub fn chain(&self, role: &str) -> Option<&Chain> {
		self.chains.iter().find(|chain| chain.role == role)
	}

	/// Checks the RFC's format rules on known fields.
	fn validate(&self) -> Result<(), Error> {
		if self.sequence > MAX_SEQUENCE {
			return Err(Error::SequenceOutOfRange(self.sequence));
		}
		if self.generation.is_empty() {
			return Err(Error::Empty("generation"));
		}
		if !is_label(&self.tld) {
			return Err(Error::InvalidTld(self.tld.clone()));
		}
		for time in [&self.expires_at, &self.valid_until].into_iter().flatten() {
			format::unix_seconds(time)?;
		}
		self.validate_numbers()?;
		self.validate_chains()?;
		self.validate_services()?;
		self.validate_extras()?;
		self.validate_fork()
	}

	/// I-JSON readers hold numbers as doubles, so every integer must be exact in one.
	fn validate_numbers(&self) -> Result<(), Error> {
		let numbers = self
			.assets
			.iter()
			.flatten()
			.map(|asset| ("assets[].id", asset.id))
			.chain(self.services.iter().filter_map(|service| Some(("chainId", service.chain_id?))))
			.chain(
				self.chains.iter().filter_map(|chain| {
					Some(("forkedAt.number", chain.forked_at.as_ref()?.number))
				}),
			)
			.chain(self.coinage.iter().map(|coinage| ("coinage.instance", coinage.instance)));
		for (field, number) in numbers {
			if number > MAX_SEQUENCE {
				return Err(Error::NumberOutOfRange(field));
			}
		}
		Ok(())
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
			if (service.kind == "eth-rpc") != service.chain_id.is_some() {
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
			format::unix_seconds(&fork.at)?;
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

/// Reads a field that may be left out but, when present, holds a value: `null` is not absent.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
	deserializer: D,
) -> Result<Option<T>, D::Error> {
	T::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn parse(text: &str) -> Descriptor {
		Descriptor::from_json(serde_json::from_str(text).unwrap()).unwrap()
	}

	const RELAY: &str = r#"[{"role":"Relay","genesisHash":"0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92","rpc":["ws://127.0.0.1:9944"]}]"#;

	#[test]
	fn content_hash_ignores_sequence_and_key_order_but_not_content() {
		let one = parse(&format!(
			r#"{{"$v":1,"sequence":1,"generation":"g","id":"a","name":"A","network":"n","tld":"t","services":[],"chains":{RELAY}}}"#
		));
		let reordered = parse(&format!(
			r#"{{"chains":{RELAY},"services":[],"tld":"t","network":"n","name":"A","id":"a","generation":"g","sequence":9,"$v":1}}"#
		));
		assert_eq!(one.content_hash(), reordered.content_hash());
		let renamed = Descriptor { name: "B".into(), ..one.clone() };
		assert_ne!(one.content_hash(), renamed.content_hash());
	}
}
