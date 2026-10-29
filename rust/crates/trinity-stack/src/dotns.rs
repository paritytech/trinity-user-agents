//! A stack's dotNS contracts, read from its Asset Hub in the three steps the RFC describes. The
//! caller makes every read; [`DotnsLookup`] says which and judges the answers.

use crate::{Block, Error, format};
use serde::{Deserialize, Serialize, Serializer};
use std::collections::BTreeMap;

/// The protocol registry's address keys, as dotNS's `DotnsConstants` names them.
pub const DOTNS_KEYS: [&str; 16] = [
	"registrar",
	"controller",
	"registry",
	"reverseResolver",
	"popRules",
	"storeFactory",
	"resolver",
	"contentResolver",
	"popController",
	"popResolver",
	"popLens",
	"nameEscrow",
	"multicall3",
	"create3Factory",
	"nameWhitelist",
	"costModel",
];

/// `DotnsGateway.DispatcherAddress`: `twox128("DotnsGateway") ++ twox128("DispatcherAddress")`.
const DISPATCHER_ADDRESS_KEY: [u8; 32] = [
	0x57, 0xca, 0x90, 0x18, 0xd6, 0x48, 0xb4, 0x0a, 0x82, 0x86, 0xa4, 0x71, 0xe8, 0x2c, 0x38, 0x08,
	0xf3, 0x1c, 0xe5, 0xec, 0x27, 0x8d, 0xb3, 0xdf, 0xf6, 0x30, 0x43, 0x33, 0xf0, 0x01, 0x93, 0x02,
];
const PROTOCOL_REGISTRY: [u8; 4] = [0x76, 0x56, 0x41, 0x9f];
const TARGET: [u8; 4] = [0xcc, 0x1f, 0x2a, 0xfa];
const REGISTRY_GET: [u8; 4] = [0x8e, 0xaa, 0x6a, 0xc0];

/// A stack's dotNS contracts, as its Asset Hub's gateway and protocol registry name them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Dotns {
	/// The contract `DotnsGateway` dispatches to.
	pub dispatcher: String,
	/// The protocol registry, which names every other dotNS contract.
	pub protocol_registry: String,
	/// Registry keys to addresses. Keys the registry leaves unset are absent.
	pub addresses: BTreeMap<String, String>,
}

/// What a reader keeps of a lookup. It serves only descriptors of its `generation`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[serde(deny_unknown_fields)]
pub struct DotnsRecord {
	/// The `generation` of the descriptor the lookup ran for.
	pub generation: String,
	/// The Asset Hub block every read was made at.
	pub at: Block,
	/// `None` when the stack's Asset Hub has no dotNS gateway.
	pub dotns: Option<Dotns>,
}

impl DotnsRecord {
	/// Reads a record as [`DotnsRecord::to_json`] wrote it, checking every address.
	pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
		let record: Self =
			serde_json::from_slice(bytes).map_err(|error| Error::Malformed(error.to_string()))?;
		record.check()?;
		Ok(record)
	}

	/// The record as a reader keeps it, in a store or in a project's build copy.
	pub fn to_json(&self) -> Vec<u8> {
		let mut json = serde_json::to_vec_pretty(self).expect("a record serialises");
		json.push(b'\n');
		json
	}

	/// Fails when an address or the block hash is not lowercase 0x-prefixed hex of its length.
	pub fn check(&self) -> Result<(), Error> {
		format::hex_bytes::<32>("at.hash", &self.at.hash)?;
		let Some(dotns) = &self.dotns else { return Ok(()) };
		format::hex_bytes::<20>("dispatcher", &dotns.dispatcher)?;
		format::hex_bytes::<20>("protocolRegistry", &dotns.protocol_registry)?;
		for address in dotns.addresses.values() {
			format::hex_bytes::<20>("addresses", address)?;
		}
		Ok(())
	}
}

/// A contract call a lookup needs, made at its block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct DotnsCall {
	/// The contract.
	#[serde(serialize_with = "hex_string")]
	#[cfg_attr(feature = "js", tsify(type = "string"))]
	pub to: [u8; 20],
	/// The calldata.
	#[serde(serialize_with = "hex_string")]
	#[cfg_attr(feature = "js", tsify(type = "string"))]
	pub input: Vec<u8>,
}

fn hex_string<S: Serializer>(bytes: impl AsRef<[u8]>, serializer: S) -> Result<S::Ok, S::Error> {
	serializer.serialize_str(&format::to_hex(bytes.as_ref()))
}

/// How a contract answered a [`DotnsCall`]. A call that could not be made is no answer: the
/// caller stops the lookup instead, so an unreachable node never reads as a missing contract.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DotnsAnswer {
	/// The call returned this data.
	Returned(Vec<u8>),
	/// The call reverted.
	Reverted,
}

/// What a [`DotnsLookup`] needs next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify), tsify(hashmap_as_object))]
#[serde(rename_all = "camelCase")]
pub enum DotnsStep {
	/// The value at this Asset Hub storage key; answer with [`DotnsLookup::storage`].
	Storage(
		#[serde(serialize_with = "hex_string")]
		#[cfg_attr(feature = "js", tsify(type = "string"))]
		[u8; 32],
	),
	/// These calls' answers, in order; answer with [`DotnsLookup::answer`]. They do not depend
	/// on each other, so a caller can batch them.
	Calls(Vec<DotnsCall>),
	/// The lookup is over.
	Done(DotnsRecord),
}

/// Reads a stack's dotNS contracts, one [`DotnsStep`] at a time, all at one block.
///
/// ```
/// use trinity_stack::{Block, DotnsAnswer, DotnsLookup, DotnsStep};
///
/// # fn read_storage(_: [u8; 32]) -> Option<Vec<u8>> { None }
/// # fn call(_: &trinity_stack::DotnsCall) -> DotnsAnswer { DotnsAnswer::Reverted }
/// let at = Block { number: 182_044, hash: format!("0x{}", "ab".repeat(32)) };
/// let mut lookup = DotnsLookup::new("2026-10-26T22:19:25Z".into(), at)?;
/// let record = loop {
///     match lookup.next() {
///         DotnsStep::Storage(key) => lookup.storage(read_storage(key).as_deref())?,
///         DotnsStep::Calls(calls) => lookup.answer(&calls.iter().map(call).collect::<Vec<_>>())?,
///         DotnsStep::Done(record) => break record,
///     }
/// };
/// # Ok::<(), trinity_stack::Error>(())
/// ```
#[derive(Clone, Debug)]
pub struct DotnsLookup {
	generation: String,
	at: Block,
	state: State,
}

#[derive(Clone, Debug)]
enum State {
	Gateway,
	ProtocolRegistry { dispatcher: [u8; 20] },
	Target { dispatcher: [u8; 20] },
	TargetRegistry { dispatcher: [u8; 20], controller: [u8; 20] },
	Addresses { dispatcher: [u8; 20], registry: [u8; 20] },
	Done(Option<Dotns>),
}

impl DotnsLookup {
	/// A lookup for a descriptor of `generation`, whose every read the caller makes at `at`.
	/// Fails when `at`'s hash is not lowercase 0x-prefixed hex, which no record may carry.
	pub fn new(generation: String, at: Block) -> Result<Self, Error> {
		format::hex_bytes::<32>("at.hash", &at.hash)?;
		Ok(Self { generation, at, state: State::Gateway })
	}

	/// What the lookup needs next.
	pub fn next(&self) -> DotnsStep {
		let call = |to: [u8; 20], input: &[u8]| DotnsCall { to, input: input.to_vec() };
		match &self.state {
			State::Gateway => DotnsStep::Storage(DISPATCHER_ADDRESS_KEY),
			State::ProtocolRegistry { dispatcher } => {
				DotnsStep::Calls(vec![call(*dispatcher, &PROTOCOL_REGISTRY)])
			},
			State::Target { dispatcher } => DotnsStep::Calls(vec![call(*dispatcher, &TARGET)]),
			State::TargetRegistry { controller, .. } => {
				DotnsStep::Calls(vec![call(*controller, &PROTOCOL_REGISTRY)])
			},
			State::Addresses { registry, .. } => DotnsStep::Calls(
				DOTNS_KEYS.iter().map(|key| call(*registry, &registry_get(key))).collect(),
			),
			State::Done(dotns) => DotnsStep::Done(DotnsRecord {
				generation: self.generation.clone(),
				at: self.at.clone(),
				dotns: dotns.clone(),
			}),
		}
	}

	/// Takes the value of [`DotnsStep::Storage`]'s key, `None` when it is unset. Fails when the
	/// gateway holds something other than an address.
	pub fn storage(&mut self, value: Option<&[u8]>) -> Result<(), Error> {
		let State::Gateway = self.state else {
			return Err(Error::LookupStep("a storage value where calls were asked".into()));
		};
		self.state = match value {
			None => State::Done(None),
			Some(value) => State::ProtocolRegistry {
				dispatcher: value.try_into().map_err(|_| Error::DispatcherAddress(value.len()))?,
			},
		};
		Ok(())
	}

	/// Takes the answers to [`DotnsStep::Calls`], in the order they were asked. Fails when a
	/// contract answers in a way no dotNS deployment does, and then stays on this step, so a
	/// caller drops the lookup.
	pub fn answer(&mut self, answers: &[DotnsAnswer]) -> Result<(), Error> {
		let DotnsStep::Calls(calls) = self.next() else {
			return Err(Error::LookupStep("call answers where none were asked".into()));
		};
		if answers.len() != calls.len() {
			return Err(Error::LookupStep(format!(
				"{} answers to {} calls",
				answers.len(),
				calls.len()
			)));
		}
		self.state = match (&self.state, &answers[0]) {
			(State::ProtocolRegistry { dispatcher }, DotnsAnswer::Returned(data)) => {
				State::Addresses { dispatcher: *dispatcher, registry: address(data, &calls[0])? }
			},
			(State::ProtocolRegistry { dispatcher }, DotnsAnswer::Reverted) => {
				State::Target { dispatcher: *dispatcher }
			},
			(State::Target { dispatcher }, DotnsAnswer::Returned(data)) => State::TargetRegistry {
				dispatcher: *dispatcher,
				controller: address(data, &calls[0])?,
			},
			(State::Target { dispatcher }, DotnsAnswer::Reverted) => {
				return Err(Error::UnknownDispatcher(format::to_hex(dispatcher)));
			},
			(State::TargetRegistry { dispatcher, .. }, DotnsAnswer::Returned(data)) => {
				State::Addresses { dispatcher: *dispatcher, registry: address(data, &calls[0])? }
			},
			(State::Addresses { dispatcher, registry }, _) => {
				let mut addresses = BTreeMap::new();
				for ((key, call), answer) in DOTNS_KEYS.iter().zip(&calls).zip(answers) {
					let DotnsAnswer::Returned(data) = answer else { return Err(reverted(call)) };
					let value = address(data, call)?;
					if value != [0; 20] {
						addresses.insert(key.to_string(), format::to_hex(&value));
					}
				}
				State::Done(Some(Dotns {
					dispatcher: format::to_hex(dispatcher),
					protocol_registry: format::to_hex(registry),
					addresses,
				}))
			},
			(_, DotnsAnswer::Reverted) => return Err(reverted(&calls[0])),
			(State::Gateway | State::Done(_), _) => unreachable!("no calls are asked here"),
		};
		Ok(())
	}
}

/// `get(bytes32)` for a key, which Solidity's `bytes32("...")` holds padded on the right.
fn registry_get(key: &str) -> Vec<u8> {
	let mut input = REGISTRY_GET.to_vec();
	input.extend_from_slice(key.as_bytes());
	input.resize(REGISTRY_GET.len() + 32, 0);
	input
}

/// The address a call returns as its one ABI word: twelve zero bytes, then the address.
fn address(data: &[u8], call: &DotnsCall) -> Result<[u8; 20], Error> {
	match data.split_at_checked(12) {
		Some((padding, address)) if data.len() == 32 && padding == [0; 12] => {
			Ok(address.try_into().expect("20 bytes follow 12 in 32"))
		},
		_ => Err(Error::NotAnAddress {
			contract: format::to_hex(&call.to),
			call: format::to_hex(&call.input),
			returned: format::to_hex(data),
		}),
	}
}

fn reverted(call: &DotnsCall) -> Error {
	Error::Reverted { contract: format::to_hex(&call.to), call: format::to_hex(&call.input) }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn a_registry_key_is_its_bytes_padded_on_the_right() {
		assert_eq!(
			format::to_hex(&registry_get("registrar")),
			format!("0x8eaa6ac0{}{}", hex::encode("registrar"), "0".repeat(46))
		);
	}

	#[test]
	fn answering_the_wrong_step_is_refused_and_changes_nothing() {
		let at = Block { number: 1, hash: format!("0x{}", "00".repeat(32)) };
		let mut lookup = DotnsLookup::new("g".into(), at).unwrap();
		assert_eq!(lookup.answer(&[]).map_err(|error| error.code()), Err("lookup-step"));
		lookup.storage(Some(&[0xd1; 20])).unwrap();
		assert_eq!(lookup.storage(None).map_err(|error| error.code()), Err("lookup-step"));
		assert_eq!(lookup.answer(&[]).map_err(|error| error.code()), Err("lookup-step"));
		let DotnsStep::Calls(calls) = lookup.next() else { panic!("asks for a call") };
		assert_eq!(calls, vec![DotnsCall { to: [0xd1; 20], input: PROTOCOL_REGISTRY.to_vec() }]);
	}
}
