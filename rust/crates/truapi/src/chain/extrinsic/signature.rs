//! [`VerifySignatureExtension`]: the signer's signature as an ordinary
//! transaction extension.

use subxt::{
	config::{substrate::SubstrateConfig, transaction_extensions::VerifySignatureDetails},
	tx::Signer,
};

use super::{
	ExtensionError, InheritedImplication, TransactionExtension, well_known::VERIFY_MULTI_SIGNATURE,
};

/// Signs Substrate payloads: gives an account id and signs bytes. `Sync`, so a
/// build holding one can run on any thread.
pub type SubstrateSigner = dyn Signer<SubstrateConfig> + Sync;

/// `VerifyMultiSignature`: `Signed` with a signer's signature over the
/// extrinsic's implication, or `Disabled` when another extension authorizes
/// the extrinsic. A V5 extrinsic carries it as an extension; a V4 one takes
/// the signer out of the pipeline and puts its signature in the preamble.
pub struct VerifySignatureExtension<'a> {
	signer: Option<&'a SubstrateSigner>,
}

impl<'a> VerifySignatureExtension<'a> {
	/// Signs with `signer`.
	pub fn signed(signer: &'a SubstrateSigner) -> Self {
		Self { signer: Some(signer) }
	}

	/// Carries no signature.
	pub fn disabled() -> Self {
		Self { signer: None }
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
		let Some(signer) = self.signer else {
			return Ok(VerifySignatureDetails::Disabled);
		};
		let signature = signer.sign(&implication.signer_payload());
		Ok(VerifySignatureDetails::Signed { signature, account: signer.account_id() })
	}

	fn signer(&self) -> Option<&SubstrateSigner> {
		self.signer
	}
}
