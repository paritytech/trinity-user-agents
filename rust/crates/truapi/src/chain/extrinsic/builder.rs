//! [`ExtrinsicBuilder`]: what a caller asks one extrinsic to contain.

use subxt::{
	ArcMetadata,
	ext::{frame_decode::extrinsics::encode_call_data, scale_encode::EncodeAsFields},
	tx::Payload,
	utils::{AccountId32, H256},
};

use super::{
	ExtrinsicBuildError, ExtrinsicVersion, TransactionExtension,
	extension::SuppliedExtension,
	signature::{SubstrateSigner, VerifySignatureExtension},
	well_known::restrict_origins,
};
use crate::chain::HashAndNumber;

mod assembly;

pub use assembly::assemble;

/// How many blocks an extrinsic stays valid for unless the caller says
/// otherwise.
pub const DEFAULT_MORTALITY_PERIOD: u64 = 64;

/// The runtime at the block a build is born at.
#[derive(Clone, Debug)]
pub struct RuntimeAtBirth {
	/// The genesis hash the caller configured, never one a node reports.
	pub genesis: H256,
	/// The block the extrinsic's era starts at.
	pub birth: HashAndNumber,
	/// The runtime's spec version at the birth block.
	pub spec_version: u32,
	/// The runtime's transaction version at the birth block.
	pub transaction_version: u32,
	/// The runtime's metadata at the birth block.
	pub metadata: ArcMetadata,
}

/// What a caller asks one extrinsic to contain.
///
/// The setters record what they are given. A problem with any of it, such as
/// a call that does not encode, is the build's error.
pub struct ExtrinsicBuilder<'a> {
	runtime: RuntimeAtBirth,
	call: Option<Result<Vec<u8>, ExtrinsicBuildError>>,
	extensions: Vec<SuppliedExtension<'a>>,
	signature: Option<VerifySignatureExtension<'a>>,
	nonce: Option<u64>,
	period: u64,
	version: Option<ExtrinsicVersion>,
}

/// A builder for an extrinsic born at `runtime`'s birth block.
pub fn start_build<'a>(runtime: RuntimeAtBirth) -> ExtrinsicBuilder<'a> {
	ExtrinsicBuilder {
		runtime,
		call: None,
		extensions: Vec::new(),
		signature: None,
		nonce: None,
		period: DEFAULT_MORTALITY_PERIOD,
		version: None,
	}
}

/// Whether the caller pinned the extrinsic version, so the host's default is
/// not needed.
pub fn pins_version(builder: &ExtrinsicBuilder<'_>) -> bool {
	builder.version.is_some()
}

/// The account whose next nonce the build has to read: the signer's, unless
/// the caller set the nonce or there is no signer.
pub fn account_needing_nonce(builder: &ExtrinsicBuilder<'_>) -> Option<AccountId32> {
	builder.nonce.is_none().then_some(builder.signature.as_ref()?.account())
}

impl<'a> ExtrinsicBuilder<'a> {
	/// Sets the call from a subxt payload. Replaces any earlier call.
	pub fn with_call(&mut self, call: impl Payload) -> &mut Self {
		let encoded = self.encode_call(call.pallet_name(), call.call_name(), call.call_data());
		self.call = Some(encoded);
		self
	}

	/// Sets the call `pallet.call` with arguments encoded against metadata.
	/// Replaces any earlier call.
	pub fn with_call_args(
		&mut self,
		pallet: &str,
		call: &str,
		args: impl EncodeAsFields,
	) -> &mut Self {
		self.call = Some(self.encode_call(pallet, call, &args));
		self
	}

	fn encode_call(
		&self,
		pallet: &str,
		call: &str,
		args: &impl EncodeAsFields,
	) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let metadata = &self.runtime.metadata;
		encode_call_data(pallet, call, args, &**metadata, metadata.types()).map_err(|error| {
			ExtrinsicBuildError::Call {
				pallet: pallet.to_string(),
				call: call.to_string(),
				reason: error.to_string(),
			}
		})
	}

	/// Sets an already-encoded call: pallet index, call index, arguments.
	/// Replaces any earlier call.
	pub fn with_call_bytes(&mut self, call: Vec<u8>) -> &mut Self {
		self.call = Some(Ok(call));
		self
	}

	/// Supplies an extension's values. A supplied extension takes precedence
	/// over any value the builder would fill in.
	pub fn with_extension<E: TransactionExtension + 'a>(&mut self, extension: E) -> &mut Self {
		self.extensions.push(SuppliedExtension::new(extension));
		self
	}

	/// Adds `VerifyMultiSignature`, signed by `signer`. A V5 extrinsic carries
	/// it as an extension; a V4 one carries its signature in the preamble.
	pub fn with_signer(&mut self, signer: &'a SubstrateSigner) -> &mut Self {
		self.signature = Some(VerifySignatureExtension::new(signer));
		self
	}

	/// Overrides the nonce. Without it, a signed build reads the signer's
	/// next index from the transaction pool, and an unsigned one uses 0.
	pub fn with_nonce(&mut self, nonce: u64) -> &mut Self {
		self.nonce = Some(nonce);
		self
	}

	/// Overrides how many blocks the extrinsic stays valid for.
	pub fn with_mortality_period(&mut self, period: u64) -> &mut Self {
		self.period = period;
		self
	}

	/// Pins the extrinsic version instead of taking the host's default.
	pub fn with_extrinsic_version(&mut self, version: ExtrinsicVersion) -> &mut Self {
		self.version = Some(version);
		self
	}

	/// Sets `RestrictOrigins`, which is unrestricted unless set.
	pub fn with_restrict_origins(&mut self, restrict: bool) -> &mut Self {
		self.with_extension(restrict_origins(restrict))
	}
}
