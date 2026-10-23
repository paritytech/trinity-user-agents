//! Values the builder fills in for extensions the caller leaves unset.

use parity_scale_codec::{Compact, Encode};
use scale_info::{PortableRegistry, TypeDef};
use subxt::{
	config::{substrate::SubstrateConfig, transaction_extensions::VerifySignatureDetails},
	ext::{
		scale_decode::visitor::{IgnoreVisitor, decode_with_visitor},
		scale_encode::EncodeAsType,
	},
	utils::{AccountId32, H256, MultiSignature},
};

use super::{
	ExtensionError, InheritedImplication, TransactionExtension, error::ExtrinsicBuildError,
};
use crate::chain::Mortality;

/// The V5 signature extension's name in metadata.
pub const VERIFY_MULTI_SIGNATURE: &str = "VerifyMultiSignature";

/// Origin restriction. The builder sets it to `false` when the caller does
/// not supply it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestrictOrigins(pub bool);

impl TransactionExtension for RestrictOrigins {
	type Implicit = ();
	type Explicit = bool;

	fn name(&self) -> &str {
		"RestrictOrigins"
	}

	fn implicit(&self) {}

	fn explicit(&self, _implication: &InheritedImplication) -> Result<bool, ExtensionError> {
		Ok(self.0)
	}
}

/// What the build knows when it fills well-known extensions.
#[derive(Clone, Copy, Debug)]
pub struct ChainValues {
	/// The genesis hash the caller configured, never one a node reports.
	pub genesis: H256,
	/// The runtime's spec version at the birth block.
	pub spec_version: u32,
	/// The runtime's transaction version at the birth block.
	pub transaction_version: u32,
	/// The era the extrinsic is signed with.
	pub mortality: Mortality,
	/// The nonce `CheckNonce` carries.
	pub nonce: u64,
}

/// One extension's encoded values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotValues {
	/// What the extrinsic carries.
	pub explicit: Vec<u8>,
	/// What only the signer payload carries.
	pub implicit: Vec<u8>,
}

/// The values of an extension every caller would fill the same way, or
/// `None` when `name` is not one of them.
pub fn well_known_values(name: &str, chain: &ChainValues) -> Option<SlotValues> {
	let (explicit, implicit) = match name {
		"CheckSpecVersion" => (Vec::new(), chain.spec_version.encode()),
		"CheckTxVersion" => (Vec::new(), chain.transaction_version.encode()),
		"CheckGenesis" => (Vec::new(), chain.genesis.0.to_vec()),
		"CheckMortality" => {
			(chain.mortality.era_bytes().to_vec(), chain.mortality.birth().hash.0.to_vec())
		},
		"CheckNonce" => (Compact(chain.nonce).encode(), Vec::new()),
		// No tip, paid in the native asset.
		"ChargeAssetTxPayment" => (vec![0x00, 0x00], Vec::new()),
		"ChargeTransactionPayment" => (vec![0x00], Vec::new()),
		// `Mode::Disabled`, and no metadata hash in the signer payload.
		"CheckMetadataHash" => (vec![0x00], vec![0x00]),
		"RestrictOrigins" => (vec![0x00], Vec::new()),
		_ => return None,
	};
	Some(SlotValues { explicit, implicit })
}

/// The value an unset extension slot of type `type_id` takes: nothing for an
/// empty type, `None` for an `Option`, and no default for anything else.
pub fn unset_slot_value(type_id: u32, types: &PortableRegistry) -> Option<Vec<u8>> {
	if is_empty_type(type_id, types) {
		return Some(Vec::new());
	}
	defaults_to_none(type_id, types).then(|| vec![0x00])
}

/// Whether a type encodes to zero bytes. Every type that is not empty needs
/// at least one byte, so it cannot decode from none.
fn is_empty_type(type_id: u32, types: &PortableRegistry) -> bool {
	decode_with_visitor(&mut &[][..], type_id, types, IgnoreVisitor::new()).is_ok()
}

/// Whether a type is an `Option`, or a one-field struct wrapping one, such as
/// `AsPgas(Option<..>)`.
fn defaults_to_none(type_id: u32, types: &PortableRegistry) -> bool {
	let Some(resolved) = types.resolve(type_id) else {
		return false;
	};
	match &resolved.type_def {
		TypeDef::Variant(_) => resolved.path.segments.last().is_some_and(|name| name == "Option"),
		TypeDef::Composite(composite) if composite.fields.len() == 1 => {
			defaults_to_none(composite.fields[0].ty.id, types)
		},
		_ => false,
	}
}

/// `VerifyMultiSignature`'s value: `Signed` when there is a signature, else
/// `Disabled`, encoded by variant name against the runtime's type.
pub fn verify_signature_value(
	signature: Option<(AccountId32, MultiSignature)>,
	type_id: u32,
	types: &PortableRegistry,
) -> Result<Vec<u8>, ExtrinsicBuildError> {
	let details = match signature {
		Some((account, signature)) => {
			VerifySignatureDetails::<SubstrateConfig>::Signed { signature, account }
		},
		None => VerifySignatureDetails::<SubstrateConfig>::Disabled,
	};
	details
		.encode_as_type(type_id, types)
		.map_err(|error| ExtrinsicBuildError::ExtensionShape {
			extension: VERIFY_MULTI_SIGNATURE.to_string(),
			slot: "explicit",
			bytes: String::new(),
			reason: error.to_string(),
		})
}
