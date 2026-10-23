//! The well-known transaction extensions, as typed values, and the defaults
//! the builder uses for extensions the caller leaves unset.

use scale_info::{PortableRegistry, TypeDef};
use subxt::{
	ext::{
		scale_decode::visitor::{IgnoreVisitor, decode_with_visitor},
		scale_encode::EncodeAsType,
	},
	utils::{Era, H256},
};

use super::{
	ExtensionError, InheritedImplication, TransactionExtension, extension::SuppliedExtension,
};
use crate::chain::Mortality;

/// The V5 signature extension's name in metadata.
pub const VERIFY_MULTI_SIGNATURE: &str = "VerifyMultiSignature";

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
pub struct EncodedExtensionValues {
	/// What the extrinsic carries.
	pub explicit: Vec<u8>,
	/// What only the signer payload carries.
	pub implicit: Vec<u8>,
}

/// An extension whose values are known before the build starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstantExtension<I, E> {
	name: &'static str,
	implicit: I,
	explicit: E,
}

impl<I, E> TransactionExtension for ConstantExtension<I, E>
where
	I: EncodeAsType + Clone + Send,
	E: EncodeAsType + Clone + Send,
{
	type Implicit = I;
	type Explicit = E;

	fn name(&self) -> &str {
		self.name
	}

	fn implicit(&self) -> I {
		self.implicit.clone()
	}

	fn explicit(&self, _implication: &InheritedImplication) -> Result<E, ExtensionError> {
		Ok(self.explicit.clone())
	}
}

/// `CheckSpecVersion`: the runtime's spec version, signed but not carried.
pub fn check_spec_version(spec_version: u32) -> ConstantExtension<u32, ()> {
	ConstantExtension { name: "CheckSpecVersion", implicit: spec_version, explicit: () }
}

/// `CheckTxVersion`: the runtime's transaction version, signed but not
/// carried.
pub fn check_tx_version(transaction_version: u32) -> ConstantExtension<u32, ()> {
	ConstantExtension { name: "CheckTxVersion", implicit: transaction_version, explicit: () }
}

/// `CheckGenesis`: the chain's genesis hash, signed but not carried.
pub fn check_genesis(genesis: H256) -> ConstantExtension<H256, ()> {
	ConstantExtension { name: "CheckGenesis", implicit: genesis, explicit: () }
}

/// `CheckMortality`'s value: the runtime wraps the era in a one-field struct,
/// which `Era`'s own encoder does not look through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct MortalityEra(pub Era);

/// `CheckMortality`: the era, and the birth block's hash it is signed over.
pub fn check_mortality(mortality: Mortality) -> ConstantExtension<H256, MortalityEra> {
	let birth = mortality.birth();
	let era = MortalityEra(Era::mortal(mortality.period(), birth.number));
	ConstantExtension { name: "CheckMortality", implicit: birth.hash, explicit: era }
}

/// `CheckNonce`: the sender's nonce.
pub fn check_nonce(nonce: u64) -> ConstantExtension<(), u64> {
	ConstantExtension { name: "CheckNonce", implicit: (), explicit: nonce }
}

/// `ChargeAssetTxPayment`'s value.
#[derive(Clone, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub struct AssetTxPayment {
	/// The tip to the block author.
	pub tip: u128,
	/// The asset the fee is paid in; `None` is the native asset.
	pub asset_id: Option<()>,
}

/// `ChargeAssetTxPayment` with no tip, paid in the native asset.
pub fn charge_asset_tx_payment() -> ConstantExtension<(), AssetTxPayment> {
	let payment = AssetTxPayment { tip: 0, asset_id: None };
	ConstantExtension { name: "ChargeAssetTxPayment", implicit: (), explicit: payment }
}

/// `ChargeTransactionPayment` with no tip.
pub fn charge_transaction_payment() -> ConstantExtension<(), u128> {
	ConstantExtension { name: "ChargeTransactionPayment", implicit: (), explicit: 0 }
}

/// `CheckMetadataHash`'s mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum MetadataHashMode {
	/// The extrinsic does not commit to a metadata hash.
	Disabled,
}

/// `CheckMetadataHash` disabled: no metadata hash in the signed content.
pub fn check_metadata_hash_disabled() -> ConstantExtension<Option<[u8; 32]>, MetadataHashMode> {
	let mode = MetadataHashMode::Disabled;
	ConstantExtension { name: "CheckMetadataHash", implicit: None, explicit: mode }
}

/// `RestrictOrigins`: whether the call's origin is restricted.
pub fn restrict_origins(restrict: bool) -> ConstantExtension<(), bool> {
	ConstantExtension { name: "RestrictOrigins", implicit: (), explicit: restrict }
}

/// `VerifyMultiSignature`'s value when nothing is signed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EncodeAsType)]
#[encode_as_type(crate_path = "subxt::ext::scale_encode")]
pub enum UnsignedVerification {
	/// No signature; another extension authorizes the extrinsic.
	Disabled,
}

/// `VerifyMultiSignature` disabled, leaving authorization to another
/// extension.
pub fn verify_signature_disabled() -> ConstantExtension<(), UnsignedVerification> {
	let disabled = UnsignedVerification::Disabled;
	ConstantExtension { name: VERIFY_MULTI_SIGNATURE, implicit: (), explicit: disabled }
}

/// The extension the builder adds for `name` when the caller leaves it
/// unset, or `None` when `name` has no well-known default.
pub fn default_extension(name: &str, chain: &ChainValues) -> Option<SuppliedExtension<'static>> {
	let extension = match name {
		"CheckSpecVersion" => SuppliedExtension::new(check_spec_version(chain.spec_version)),
		"CheckTxVersion" => SuppliedExtension::new(check_tx_version(chain.transaction_version)),
		"CheckGenesis" => SuppliedExtension::new(check_genesis(chain.genesis)),
		"CheckMortality" => SuppliedExtension::new(check_mortality(chain.mortality)),
		"CheckNonce" => SuppliedExtension::new(check_nonce(chain.nonce)),
		"ChargeAssetTxPayment" => SuppliedExtension::new(charge_asset_tx_payment()),
		"ChargeTransactionPayment" => SuppliedExtension::new(charge_transaction_payment()),
		"CheckMetadataHash" => SuppliedExtension::new(check_metadata_hash_disabled()),
		"RestrictOrigins" => SuppliedExtension::new(restrict_origins(false)),
		VERIFY_MULTI_SIGNATURE => SuppliedExtension::new(verify_signature_disabled()),
		_ => return None,
	};
	Some(extension)
}

/// The value an unset extension value of type `type_id` takes: nothing for an
/// empty type, `None` for an `Option`, and no default for anything else.
pub fn unset_value_default(type_id: u32, types: &PortableRegistry) -> Option<Vec<u8>> {
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
