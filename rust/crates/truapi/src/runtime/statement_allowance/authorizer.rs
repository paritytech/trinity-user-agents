//! Allowance extrinsics authorized by a ring proof, built through the core's
//! [`ExtrinsicService`].

use subxt::{
	ext::scale_encode::EncodeAsFields,
	utils::{Encoded, H256},
};

use super::StatementAllowanceError;
use crate::chain::{
	ExtensionError, ExtrinsicBuildError, ExtrinsicService, ExtrinsicVersion, InheritedImplication,
	TransactionExtension,
};

/// An authorizing extension whose explicit value proves the hash of its
/// implication, through `prove`.
pub struct ProofAuthorizer<F> {
	name: &'static str,
	prove: F,
}

impl<F> ProofAuthorizer<F>
where
	F: Fn(&[u8; 32]) -> Result<Vec<u8>, StatementAllowanceError> + Send,
{
	/// The extension `name`, whose encoded explicit value `prove` builds from
	/// the implication hash.
	pub fn new(name: &'static str, prove: F) -> Self {
		Self { name, prove }
	}
}

impl<F> TransactionExtension for ProofAuthorizer<F>
where
	F: Fn(&[u8; 32]) -> Result<Vec<u8>, StatementAllowanceError> + Send,
{
	type Implicit = ();
	type Explicit = Encoded;

	fn name(&self) -> &str {
		self.name
	}

	fn implicit(&self) {}

	fn explicit(&self, implication: &InheritedImplication) -> Result<Encoded, ExtensionError> {
		(self.prove)(&implication.hash()).map(Encoded).map_err(ExtensionError::new)
	}
}

/// The service and the chain an allowance extrinsic is built for.
#[derive(Clone, Copy)]
pub struct ExtrinsicTarget<'a> {
	/// Builds the extrinsic.
	pub extrinsics: &'a ExtrinsicService,
	/// The chain's genesis as the host configured it.
	pub genesis: H256,
}

impl ExtrinsicTarget<'_> {
	/// Builds `pallet.call(args)` authorized by `authorizer`, mortal from the
	/// chain's finalized head, on the newest pipeline that declares the
	/// authorizer. Returns the encoded extrinsic.
	pub async fn build_authorized<Args, F>(
		self,
		pallet: &str,
		call: &str,
		args: Args,
		authorizer: ProofAuthorizer<F>,
	) -> Result<Vec<u8>, StatementAllowanceError>
	where
		Args: EncodeAsFields + Send,
		F: Fn(&[u8; 32]) -> Result<Vec<u8>, StatementAllowanceError> + Send,
	{
		let version = ExtrinsicVersion::newest_declaring(authorizer.name);
		let built = self
			.extrinsics
			.build_mortal_extrinsic(self.genesis, |builder| {
				builder.with_call_args(pallet, call, args);
				builder.with_extension(authorizer).with_extrinsic_version(version);
			})
			.await
			.map_err(allowance_error)?;
		Ok(built.extrinsic.bytes().to_vec())
	}
}

/// The allowance error a build failed with: the authorizer's own error when
/// the proof failed, else the build error.
fn allowance_error(error: ExtrinsicBuildError) -> StatementAllowanceError {
	let ExtrinsicBuildError::Extension { extension, source } = error else {
		return StatementAllowanceError::Extrinsic(error);
	};
	source.downcast::<StatementAllowanceError>().unwrap_or_else(|source| {
		StatementAllowanceError::Extrinsic(ExtrinsicBuildError::Extension { extension, source })
	})
}
