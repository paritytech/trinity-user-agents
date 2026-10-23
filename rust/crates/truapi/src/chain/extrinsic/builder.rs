//! [`ExtrinsicBuilder`]: what a caller asks one extrinsic to contain.

use subxt::{
	ArcMetadata, Metadata,
	ext::{frame_decode::extrinsics::encode_call_data, scale_encode::EncodeAsFields},
	tx::{Payload, StaticPayload},
	utils::{AccountId32, H256},
};

use super::{
	ExtrinsicBuildError, ExtrinsicVersion, TransactionExtension,
	extension::SuppliedExtension,
	signature::{SubstrateSigner, VerifySignatureExtension},
	well_known::{VERIFY_MULTI_SIGNATURE, restrict_origins},
};
use crate::chain::{HashAndNumber, MortalExtrinsic};

mod assembly;

use assembly::assemble;

/// How many blocks an extrinsic stays valid for unless the caller says
/// otherwise.
pub const DEFAULT_MORTALITY_PERIOD: u64 = 64;

/// The runtime at the block a build is born at.
#[derive(Clone, Debug)]
pub struct RuntimeAtBirth {
	/// The block the extrinsic's era starts at.
	pub birth: HashAndNumber,
	/// The runtime's spec version at the birth block.
	pub spec_version: u32,
	/// The runtime's transaction version at the birth block.
	pub transaction_version: u32,
	/// The runtime's metadata at the birth block.
	pub metadata: ArcMetadata,
}

/// What a build reads from the chain and the host.
#[async_trait::async_trait]
pub trait BuildSource: Sync {
	/// The runtime at the chain's finalized head, which the extrinsic is born
	/// at.
	async fn runtime_at_finalized_head(
		&self,
		genesis: H256,
	) -> Result<RuntimeAtBirth, ExtrinsicBuildError>;

	/// The host's pipeline for the chain, or `None` when the host builds it
	/// as V4.
	async fn host_extension_version(
		&self,
		genesis: H256,
	) -> Result<Option<u8>, ExtrinsicBuildError>;

	/// The next nonce of the signer `account`.
	async fn account_nonce(
		&self,
		genesis: H256,
		account: AccountId32,
	) -> Result<u64, ExtrinsicBuildError>;
}

/// A call of any payload type, kept unencoded until the build is assembled.
/// `Payload` cannot be boxed itself, since its call data type differs per call.
trait AnyCall: Send {
	/// The call encoded against `metadata`.
	fn encode(&self, metadata: &Metadata) -> Result<Vec<u8>, ExtrinsicBuildError>;
}

impl<P: Payload + Send> AnyCall for P {
	fn encode(&self, metadata: &Metadata) -> Result<Vec<u8>, ExtrinsicBuildError> {
		let (pallet, call) = (self.pallet_name(), self.call_name());
		encode_call_data(pallet, call, self.call_data(), metadata, metadata.types()).map_err(
			|error| ExtrinsicBuildError::Call {
				pallet: pallet.to_string(),
				call: call.to_string(),
				reason: error.to_string(),
			},
		)
	}
}

/// What a caller asks one extrinsic to contain.
///
/// The setters record what they are given. A problem with any of it, such as
/// a call that does not encode, is the build's error.
pub struct ExtrinsicBuilder<'a> {
	source: &'a dyn BuildSource,
	/// The genesis hash the caller configured, never one a node reports.
	genesis: H256,
	call: Option<Box<dyn AnyCall + 'a>>,
	extensions: Vec<SuppliedExtension<'a>>,
	nonce: Option<u64>,
	period: u64,
	version: Option<ExtrinsicVersion>,
}

/// A builder for an extrinsic on the chain `genesis`, which reads what it
/// needs from `source` when built.
pub fn start_build(source: &dyn BuildSource, genesis: H256) -> ExtrinsicBuilder<'_> {
	ExtrinsicBuilder {
		source,
		genesis,
		call: None,
		extensions: Vec::new(),
		nonce: None,
		period: DEFAULT_MORTALITY_PERIOD,
		version: None,
	}
}

impl<'a> ExtrinsicBuilder<'a> {
	/// Sets the call from a subxt payload. Replaces any earlier call.
	pub fn with_call(mut self, call: impl Payload + Send + 'a) -> Self {
		self.call = Some(Box::new(call));
		self
	}

	/// Sets the call `pallet.call` with arguments encoded against metadata.
	/// Replaces any earlier call.
	pub fn with_call_args(
		self,
		pallet: &str,
		call: &str,
		args: impl EncodeAsFields + Send + 'a,
	) -> Self {
		self.with_call(StaticPayload::new(pallet, call, args))
	}

	/// Supplies an extension's values. A supplied extension takes precedence
	/// over any value the builder would fill in.
	pub fn with_extension<E: TransactionExtension + 'a>(mut self, extension: E) -> Self {
		self.extensions.push(Box::new(extension));
		self
	}

	/// Adds `VerifyMultiSignature`, signed by `signer`. A V5 extrinsic carries
	/// it as an extension; a V4 one carries its signature in the preamble.
	pub fn with_signer(self, signer: &'a SubstrateSigner) -> Self {
		self.with_extension(VerifySignatureExtension::signed(signer))
	}

	/// Overrides the nonce. Without it, a signed build reads the signer's
	/// next index from the transaction pool, and an unsigned one uses 0.
	pub fn with_nonce(mut self, nonce: u64) -> Self {
		self.nonce = Some(nonce);
		self
	}

	/// Overrides how many blocks the extrinsic stays valid for.
	pub fn with_mortality_period(mut self, period: u64) -> Self {
		self.period = period;
		self
	}

	/// Pins the extrinsic version instead of taking the host's default.
	pub fn with_extrinsic_version(mut self, version: ExtrinsicVersion) -> Self {
		self.version = Some(version);
		self
	}

	/// Sets `RestrictOrigins`, which is unrestricted unless set.
	pub fn with_restrict_origins(self, restrict: bool) -> Self {
		self.with_extension(restrict_origins(restrict))
	}

	/// Builds the extrinsic, mortal from the chain's finalized head.
	pub async fn build(self) -> Result<MortalExtrinsic, ExtrinsicBuildError> {
		let runtime = self.source.runtime_at_finalized_head(self.genesis).await?;
		assemble(self, runtime).await
	}

	/// The signer of the supplied `VerifyMultiSignature`, when the build signs.
	fn signer(&self) -> Option<&SubstrateSigner> {
		let signature = self
			.extensions
			.iter()
			.find(|extension| extension.name() == VERIFY_MULTI_SIGNATURE)?;
		signature.signer()
	}
}
