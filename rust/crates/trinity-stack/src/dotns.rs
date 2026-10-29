//! Checks a descriptor's `dotns` against its Asset Hub, read in the three steps the RFC
//! describes. The caller makes every read; [`DotnsLookup`] says which and judges the answers.

use crate::{Descriptor, Error, format};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

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

/// A `dotns` key on which a descriptor and its chain disagree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
#[serde(rename_all = "camelCase")]
pub struct DotnsDrift {
	/// The registry key.
	pub key: String,
	/// The descriptor's address; `None` when the descriptor leaves the key out.
	pub stated: Option<String>,
	/// The registry's address; `None` when the registry leaves the key unset.
	pub on_chain: Option<String>,
}

/// Every key, in order, on which `descriptor`'s `dotns` and the addresses a [`DotnsLookup`]
/// found on its chain disagree; empty when they agree. `found` is `None` for a chain with no
/// dotNS gateway.
pub fn dotns_drift(
	descriptor: &Descriptor,
	found: Option<&BTreeMap<String, String>>,
) -> Vec<DotnsDrift> {
	let stated = descriptor.dotns.as_ref();
	let keys: BTreeSet<&String> =
		stated.into_iter().chain(found).flat_map(BTreeMap::keys).collect();
	keys.into_iter()
		.filter_map(|key| {
			let stated = stated.and_then(|addresses| addresses.get(key)).cloned();
			let on_chain = found.and_then(|addresses| addresses.get(key)).cloned();
			(stated != on_chain).then(|| DotnsDrift { key: key.clone(), stated, on_chain })
		})
		.collect()
}

/// A contract call a lookup needs, made at its block.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "js", derive(tsify::Tsify))]
pub struct DotnsCall {
	/// The contract.
	#[serde(serialize_with = "format::serialize_hex")]
	#[cfg_attr(feature = "js", tsify(type = "string"))]
	pub to: [u8; 20],
	/// The calldata.
	#[serde(serialize_with = "format::serialize_hex")]
	#[cfg_attr(feature = "js", tsify(type = "string"))]
	pub input: Vec<u8>,
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
		#[serde(serialize_with = "format::serialize_hex")]
		#[cfg_attr(feature = "js", tsify(type = "string"))]
		[u8; 32],
	),
	/// These calls' answers, in order; answer with [`DotnsLookup::answer`]. They do not depend
	/// on each other, so a caller can batch them.
	Calls(Vec<DotnsCall>),
	/// The lookup is over: the address of each key the registry sets, or `None` when Asset Hub
	/// has no dotNS gateway.
	Done(Option<BTreeMap<String, String>>),
}

/// Reads a stack's dotNS addresses, one [`DotnsStep`] at a time. The caller makes every read at
/// one block, such as the latest finalized one.
///
/// ```
/// use trinity_stack::{Descriptor, DotnsAnswer, DotnsLookup, DotnsStep, dotns_drift};
///
/// # fn read_storage(_: [u8; 32]) -> Option<Vec<u8>> { None }
/// # fn call(_: &trinity_stack::DotnsCall) -> DotnsAnswer { DotnsAnswer::Reverted }
/// # fn check(descriptor: &Descriptor) -> Result<(), trinity_stack::Error> {
/// let mut lookup = DotnsLookup::new(descriptor);
/// let found = loop {
///     match lookup.next() {
///         DotnsStep::Storage(key) => lookup.storage(read_storage(key).as_deref())?,
///         DotnsStep::Calls(calls) => lookup.answer(&calls.iter().map(call).collect::<Vec<_>>())?,
///         DotnsStep::Done(found) => break found,
///     }
/// };
/// let drift = dotns_drift(descriptor, found.as_ref());
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct DotnsLookup {
	keys: Vec<String>,
	state: State,
}

#[derive(Clone, Debug)]
enum State {
	Gateway,
	ProtocolRegistry { dispatcher: [u8; 20] },
	Target { dispatcher: [u8; 20] },
	TargetRegistry { controller: [u8; 20] },
	Addresses { registry: [u8; 20] },
	Done(Option<BTreeMap<String, String>>),
}

impl DotnsLookup {
	/// A lookup of every key in [`DOTNS_KEYS`] and every other key `descriptor`'s `dotns` has,
	/// so [`dotns_drift`] compares each address the descriptor has.
	pub fn new(descriptor: &Descriptor) -> Self {
		let stated = descriptor.dotns.iter().flat_map(BTreeMap::keys);
		let mut keys: Vec<String> = DOTNS_KEYS.map(str::to_owned).to_vec();
		keys.extend(stated.filter(|key| !DOTNS_KEYS.contains(&key.as_str())).cloned());
		Self { keys, state: State::Gateway }
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
			State::TargetRegistry { controller } => {
				DotnsStep::Calls(vec![call(*controller, &PROTOCOL_REGISTRY)])
			},
			State::Addresses { registry } => DotnsStep::Calls(
				self.keys.iter().map(|key| call(*registry, &registry_get(key))).collect(),
			),
			State::Done(found) => DotnsStep::Done(found.clone()),
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
			(
				State::ProtocolRegistry { .. } | State::TargetRegistry { .. },
				DotnsAnswer::Returned(data),
			) => State::Addresses { registry: address(data, &calls[0])? },
			(State::ProtocolRegistry { dispatcher }, DotnsAnswer::Reverted) => {
				State::Target { dispatcher: *dispatcher }
			},
			(State::Target { .. }, DotnsAnswer::Returned(data)) => {
				State::TargetRegistry { controller: address(data, &calls[0])? }
			},
			(State::Target { dispatcher }, DotnsAnswer::Reverted) => {
				return Err(Error::UnknownDispatcher(format::to_hex(dispatcher)));
			},
			(State::Addresses { .. }, _) => {
				let mut addresses = BTreeMap::new();
				for ((key, call), answer) in self.keys.iter().zip(&calls).zip(answers) {
					let DotnsAnswer::Returned(data) = answer else { return Err(reverted(call)) };
					let value = address(data, call)?;
					if value != [0; 20] {
						addresses.insert(key.clone(), format::to_hex(&value));
					}
				}
				State::Done(Some(addresses))
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
		let descriptor: Descriptor = serde_json::from_str(r#"{"$v":1,"sequence":1,"generation":"g","id":"a","name":"A","network":"n","tld":"dot","services":[],"chains":[{"role":"Relay","genesisHash":"0x860145753657e73c29b9388ffa0a8aebc643ea87434b4b271b6c3c3cc9e6bf92","rpc":["wss://relay.example"]}]}"#).unwrap();
		let mut lookup = DotnsLookup::new(&descriptor);
		assert_eq!(lookup.answer(&[]).map_err(|error| error.code()), Err("lookup-step"));
		lookup.storage(Some(&[0xd1; 20])).unwrap();
		assert_eq!(lookup.storage(None).map_err(|error| error.code()), Err("lookup-step"));
		assert_eq!(lookup.answer(&[]).map_err(|error| error.code()), Err("lookup-step"));
		let DotnsStep::Calls(calls) = lookup.next() else { panic!("asks for a call") };
		assert_eq!(calls, vec![DotnsCall { to: [0xd1; 20], input: PROTOCOL_REGISTRY.to_vec() }]);
	}
}
