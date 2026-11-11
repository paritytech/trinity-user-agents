//! Wallet-local identity proofs and authoritative dotNS metadata refresh.

use super::WalletAccountHolder;
use crate::{
	host_logic::{
		attestation, dotns_gateway, features, product_account::derive_lite_person_ring_vrf_entropy,
	},
	runtime::{
		authority::{AuthorityError, AuthoritySession},
		identity,
	},
};
use serde::Serialize;
use truapi::latest::{ChainIdentifier, GenericError};

/// Verified metadata for the active network's UID account.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalIdentity {
	/// Canonical lowercase 0x-prefixed 32-byte account identifier.
	pub identity_account_id: String,
	/// Lite username confirmed by direct on-chain account lookup.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub lite_username: Option<String>,
}

/// Opaque activation fence used throughout an identity backend handshake.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalIdentityContext {
	/// Generation token; not an account identifier or secret.
	pub activation_id: String,
	/// Canonical account identifier that every result must belong to.
	pub identity_account_id: String,
}

fn error(reason: impl ToString) -> GenericError {
	GenericError { reason: reason.to_string() }
}

fn unavailable(reason: impl ToString) -> AuthorityError {
	AuthorityError::Unavailable { reason: reason.to_string() }
}

impl WalletAccountHolder {
	/// Capture the current UID account and local activation generation atomically.
	pub fn local_identity_context(&self) -> Result<LocalIdentityContext, GenericError> {
		let state = self.lifecycle.lock().expect("wallet lifecycle mutex poisoned");
		if state.keys.is_none() {
			return Err(error("no active local identity"));
		}
		let account = self
			.session_state
			.current()
			.and_then(|session| session.identity_account_id)
			.ok_or_else(|| error("no active local identity"))?;
		Ok(LocalIdentityContext {
			activation_id: state.activation.to_string(),
			identity_account_id: format!("0x{}", hex::encode(account)),
		})
	}

	/// Reject identity work from a disconnected or replaced activation.
	pub fn check_identity_activation(&self, activation_id: &str) -> Result<(), GenericError> {
		self.identity_activation_session(activation_id).map(drop)
	}

	pub(super) fn identity_activation_session(
		&self,
		activation_id: &str,
	) -> Result<AuthoritySession, GenericError> {
		let state = self.lifecycle.lock().expect("wallet lifecycle mutex poisoned");
		if activation_id.parse::<u64>().ok() != Some(state.activation) || state.keys.is_none() {
			return Err(error("local identity activation changed"));
		}
		let session =
			self.session_state.current().ok_or_else(|| error("no active local identity"))?;
		if session.identity_account_id.is_none() {
			return Err(error("no active local identity"));
		}
		Ok(state.session(&session))
	}

	async fn identity_asset_hub(&self) -> Result<[u8; 32], GenericError> {
		let chains = features::supported_chains(self.services.platform.as_ref()).await?;
		features::genesis_for(&chains, ChainIdentifier::AssetHub)
			.ok_or_else(|| error("Asset Hub is not configured"))
	}

	async fn identity_signing_contexts(
		&self,
		activation_id: &str,
		asset_hub: [u8; 32],
	) -> Result<attestation::SigningContexts, GenericError> {
		use crate::runtime::statement_allowance::{fetch_metadata, rpc::RpcClient};
		self.check_identity_activation(activation_id)?;
		let people = self
			.services
			.statement_store
			.chain_client("local identity registration")
			.await
			.map_err(error)?;
		let asset_hub = self
			.services
			.chain
			.rpc_client("local identity registration", &asset_hub)
			.await
			.map_err(error)?;
		self.check_identity_activation(activation_id)?;
		let asset_hub = RpcClient::new(subxt_rpcs::RpcClient::new(asset_hub));
		let (people, asset_hub) =
			futures::try_join!(fetch_metadata(people.rpc()), fetch_metadata(&asset_hub),)
				.map_err(error)?;
		self.check_identity_activation(activation_id)?;
		Ok(attestation::SigningContexts {
			people: attestation::signing_context_from_constant(
				people.constant("PeopleLite", attestation::SIGNING_CONTEXT_CONSTANT),
			),
			asset_hub: attestation::signing_context_from_constant(
				asset_hub.constant("DotnsGateway", attestation::SIGNING_CONTEXT_CONSTANT),
			),
		})
	}

	/// Sign the backend token challenge without releasing UID secret material.
	pub fn local_identity_auth_proof(
		&self,
		activation_id: &str,
		challenge: &[u8],
	) -> Result<Vec<u8>, GenericError> {
		let session = self.identity_activation_session(activation_id)?;
		self.with_keys(&session, |keys| {
			let (client_id, proof) = attestation::sign_backend_challenge(
				&keys.entropy,
				&keys.network_suffix,
				challenge,
				b"{}",
			)
			.map_err(unavailable)?;
			let mut bytes = Vec::with_capacity(96);
			bytes.extend_from_slice(&client_id);
			bytes.extend_from_slice(&proof);
			Ok::<_, AuthorityError>(bytes)
		})
		.map_err(error)
	}

	/// Build shared registration JSON with proofs timestamped at the configured Asset Hub.
	pub async fn local_lite_registration_body(
		&self,
		activation_id: &str,
		username_base: &str,
		verifier: [u8; 32],
	) -> Result<String, GenericError> {
		if !dotns_gateway::is_registrable_full_label(username_base) ||
			username_base.len() + 3 > dotns_gateway::MAX_BASE_LABEL_LEN
		{
			return Err(error("Lite username base must contain 6 to 29 lowercase ASCII letters"));
		}
		let session = self.identity_activation_session(activation_id)?;
		let genesis = self.identity_asset_hub().await?;
		self.check_identity_activation(activation_id)?;
		let account =
			session.identity_account_id.ok_or_else(|| error("no active local identity"))?;
		let contexts = self.identity_signing_contexts(activation_id, genesis).await?;
		let signed_at = identity::registration_timestamp(&self.services.chain, genesis, account)
			.await
			.map_err(error)?;
		// Loaded before the activation lock: the browser fetches `verifiable`
		// on first use, and its operations below are then synchronous.
		let vrf = crate::runtime::vrf::load().await.map_err(error)?;
		self.with_keys(&session, |keys| {
			let vrf_entropy =
				derive_lite_person_ring_vrf_entropy(&keys.entropy, &keys.network_suffix);
			let registration = attestation::build_lite_registration_with(
				&keys.entropy,
				&keys.network_suffix,
				verifier,
				username_base,
				None,
				signed_at,
				contexts,
				vrf.member(&vrf_entropy).map_err(unavailable)?,
				|message| {
					let signature =
						vrf.sign(&vrf_entropy, message).map_err(|err| err.to_string())?;
					<[u8; 64]>::try_from(signature.as_slice())
						.map_err(|_| format!("ring-VRF signature is {} bytes", signature.len()))
				},
			)
			.map_err(unavailable)?;
			Ok::<_, AuthorityError>(
				registration.request_body(username_base, None, signed_at).to_string(),
			)
		})
		.map_err(error)
	}

	/// Install chain-verified Lite metadata only if the local activation remains current.
	pub async fn refresh_local_identity(
		&self,
		activation_id: &str,
	) -> Result<LocalIdentity, GenericError> {
		let authority = self.identity_activation_session(activation_id)?;
		let genesis = self.identity_asset_hub().await?;
		self.check_identity_activation(activation_id)?;
		let account =
			authority.identity_account_id.ok_or_else(|| error("no active local identity"))?;
		let resolved = identity::lookup_local_identity(&self.services.chain, genesis, account)
			.await
			.map_err(error)?;
		let state = self.lifecycle.lock().expect("wallet lifecycle mutex poisoned");
		let mut session =
			state.require_session(self.session_state.current(), &authority).map_err(error)?;
		let (lite_username, full_username) =
			resolved.map(|identity| identity.into_usernames()).unwrap_or_default();
		session.lite_username = lite_username;
		session.full_username = full_username;
		let result = LocalIdentity {
			identity_account_id: format!("0x{}", hex::encode(account)),
			lite_username: session.lite_username.clone(),
		};
		self.session_state.set_session(session);
		drop(state);
		Ok(result)
	}
}

#[cfg(test)]
mod tests {

	#[test]
	fn backend_proof_belongs_to_current_identity_and_rejects_reactivation() {
		let wallet = super::super::tests::unlocked_wallet(
			crate::test_support::StubPlatform::default(),
			"paseo",
			7,
		);
		let context = wallet.local_identity_context().unwrap();
		let proof = wallet.local_identity_auth_proof(&context.activation_id, b"challenge").unwrap();
		assert_eq!(proof.len(), 96);
		assert_eq!(format!("0x{}", hex::encode(&proof[..32])), context.identity_account_id);
		wallet.clear();
		assert!(wallet.local_identity_auth_proof(&context.activation_id, b"challenge").is_err());
		let activation = wallet.prepare_activation(vec![7; 32], None).unwrap();
		wallet.install(activation);
		assert!(wallet.check_identity_activation(&context.activation_id).is_err());
		let current = wallet.local_identity_context().unwrap();
		assert_ne!(context.activation_id, current.activation_id);
		assert_eq!(context.identity_account_id, current.identity_account_id);
	}
}
