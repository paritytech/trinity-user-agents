//! [`VerifySignatureExtension`]: the signer's signature as an ordinary
//! transaction extension.

use subxt::{
	config::{substrate::SubstrateConfig, transaction_extensions::VerifySignatureDetails},
	tx::Signer,
	utils::{AccountId32, MultiSignature},
};

use super::{
	ExtensionError, InheritedImplication, TransactionExtension, well_known::VERIFY_MULTI_SIGNATURE,
};

/// Signs Substrate payloads: gives an account id and signs bytes. `Sync`, so a
/// build holding one can run on any thread.
pub type SubstrateSigner = dyn Signer<SubstrateConfig> + Sync;

/// `VerifyMultiSignature` carrying the signer's signature over the
/// extrinsic's implication. A V5 extrinsic carries it as an extension; a V4
/// one takes it out of the pipeline and puts the signature in its preamble.
pub struct VerifySignatureExtension<'a> {
	signer: &'a SubstrateSigner,
}

impl<'a> VerifySignatureExtension<'a> {
	/// Signs with `signer`.
	pub fn new(signer: &'a SubstrateSigner) -> Self {
		Self { signer }
	}

	/// The signing account.
	pub fn account(&self) -> AccountId32 {
		self.signer.account_id()
	}

	/// The signature over the implication's signer payload.
	pub fn sign(&self, implication: &InheritedImplication) -> MultiSignature {
		self.signer.sign(&implication.signer_payload())
	}
}

impl TransactionExtension for VerifySignatureExtension<'_> {
	type Implicit = ();
	type Explicit = VerifySignatureDetails<SubstrateConfig>;

	fn name(&self) -> &str {
		VERIFY_MULTI_SIGNATURE
	}

	fn implicit(&self) {}

	fn explicit(
		&self,
		implication: &InheritedImplication,
	) -> Result<VerifySignatureDetails<SubstrateConfig>, ExtensionError> {
		Ok(VerifySignatureDetails::Signed {
			signature: self.sign(implication),
			account: self.account(),
		})
	}
}
