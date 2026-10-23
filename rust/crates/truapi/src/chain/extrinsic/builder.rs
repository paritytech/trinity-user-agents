//! [`ExtrinsicBuilder`]: what a caller asks one extrinsic to contain.

use subxt::{
	ArcMetadata, Metadata,
	ext::{frame_decode::extrinsics::encode_call_data, scale_encode::EncodeAsFields},
	tx::Payload,
	utils::H256,
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

/// A call kept unencoded until the build is assembled, so the builder can hold
/// calls of any argument type.
type PendingCall<'a> =
	Box<dyn FnOnce(&Metadata) -> Result<Vec<u8>, ExtrinsicBuildError> + Send + 'a>;

/// What a caller asks one extrinsic to contain.
///
/// The setters record what they are given. A problem with any of it, such as
/// a call that does not encode, is the build's error.
pub struct ExtrinsicBuilder<'a> {
	runtime: RuntimeAtBirth,
	call: Option<PendingCall<'a>>,
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

impl<'a> ExtrinsicBuilder<'a> {
	/// Sets the call from a subxt payload. Replaces any earlier call.
	pub fn with_call(&mut self, call: impl Payload + Send + 'a) -> &mut Self {
		let pending: PendingCall<'a> = Box::new(move |metadata| {
			encode_call(metadata, call.pallet_name(), call.call_name(), call.call_data())
		});
		self.call = Some(pending);
		self
	}

	/// Sets the call `pallet.call` with arguments encoded against metadata.
	/// Replaces any earlier call.
	pub fn with_call_args(
		&mut self,
		pallet: &str,
		call: &str,
		args: impl EncodeAsFields + Send + 'a,
	) -> &mut Self {
		let (pallet, call) = (pallet.to_string(), call.to_string());
		let pending: PendingCall<'a> =
			Box::new(move |metadata| encode_call(metadata, &pallet, &call, &args));
		self.call = Some(pending);
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
		self.signature = Some(VerifySignatureExtension::signed(signer));
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

	/// The signer, when the build signs.
	fn signer(&self) -> Option<&'a SubstrateSigner> {
		self.signature.as_ref()?.signer()
	}
}

/// The call `pallet.call` with `args`, encoded against `metadata`.
fn encode_call(
	metadata: &Metadata,
	pallet: &str,
	call: &str,
	args: &impl EncodeAsFields,
) -> Result<Vec<u8>, ExtrinsicBuildError> {
	encode_call_data(pallet, call, args, metadata, metadata.types()).map_err(|error| {
		ExtrinsicBuildError::Call {
			pallet: pallet.to_string(),
			call: call.to_string(),
			reason: error.to_string(),
		}
	})
}
