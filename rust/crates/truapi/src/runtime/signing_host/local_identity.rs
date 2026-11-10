//! Wallet-local identity proofs and authoritative dotNS metadata refresh.

use super::SigningHost;
use crate::{
	host_logic::{
		attestation, dotns_gateway, features, product_account::derive_lite_person_ring_vrf_entropy,
	},
	runtime::{connected_session_ui_info, identity},
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

/// The pallet that checks the candidate and consumer registration signatures.
const PEOPLE_LITE_PALLET: &str = "PeopleLite";
/// The pallet that checks the dotNS reservation signature.
const DOTNS_GATEWAY_PALLET: &str = "DotnsGateway";

impl SigningHost {
	/// Capture the current UID account and local activation generation atomically.
	pub fn local_identity_context(&self) -> Result<LocalIdentityContext, GenericError> {
		let activation = self.wallet.activation();
		let account = self
			.wallet
			.session_state()
			.current()
			.and_then(|session| session.identity_account_id)
			.ok_or_else(|| error("no active local identity"))?;
		Ok(LocalIdentityContext {
			activation_id: activation.to_string(),
			identity_account_id: format!("0x{}", hex::encode(account)),
		})
	}

	/// Reject identity work from a disconnected or replaced activation.
	pub fn check_identity_activation(&self, activation_id: &str) -> Result<(), GenericError> {
		if activation_id.parse::<u64>().ok() != Some(self.wallet.activation()) ||
			self.wallet.session_state().current().is_none()
		{
			return Err(error("local identity activation changed"));
		}
		Ok(())
	}

	async fn identity_asset_hub(&self) -> Result<[u8; 32], GenericError> {
		let chains = features::supported_chains(self.services.platform.as_ref()).await?;
		features::genesis_for(&chains, ChainIdentifier::AssetHub)
			.ok_or_else(|| error("Asset Hub is not configured"))
	}

	/// The contexts the People and Asset Hub pallets check registration
	/// signatures in, each read from that chain's `SigningContext` constant.
	async fn registration_signing_contexts(
		&self,
		asset_hub: [u8; 32],
	) -> Result<attestation::SigningContexts, GenericError> {
		let chains = features::supported_chains(self.services.platform.as_ref()).await?;
		let people = features::genesis_for(&chains, ChainIdentifier::People)
			.ok_or_else(|| error("People chain is not configured"))?;
		Ok(attestation::SigningContexts {
			people: self.signing_context_of(people, PEOPLE_LITE_PALLET).await?,
			asset_hub: self.signing_context_of(asset_hub, DOTNS_GATEWAY_PALLET).await?,
		})
	}

	async fn signing_context_of(
		&self,
		genesis: [u8; 32],
		pallet: &str,
	) -> Result<Option<[u8; 32]>, GenericError> {
		let block = self
			.services
			.chain
			.current_block(&genesis)
			.await
			.map_err(|failure| error(format!("read {pallet} metadata: {failure}")))?;
		let metadata = block.metadata();
		Ok(attestation::signing_context_from_constant(
			metadata
				.pallet_by_name(pallet)
				.and_then(|pallet| pallet.constant_by_name(attestation::SIGNING_CONTEXT_CONSTANT))
				.map(|constant| constant.value()),
		))
	}

	/// Sign the backend token challenge without releasing UID secret material.
	pub fn local_identity_auth_proof(
		&self,
		activation_id: &str,
		challenge: &[u8],
	) -> Result<Vec<u8>, GenericError> {
		self.check_identity_activation(activation_id)?;
		let entropy = self.wallet.root_entropy().map_err(error)?;
		let (client_id, proof) =
			attestation::sign_backend_challenge(&entropy, self.network_suffix(), challenge, b"{}")
				.map_err(error)?;
		let mut bytes = Vec::with_capacity(96);
		bytes.extend_from_slice(&client_id);
		bytes.extend_from_slice(&proof);
		Ok(bytes)
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
		self.check_identity_activation(activation_id)?;
		let genesis = self.identity_asset_hub().await?;
		self.check_identity_activation(activation_id)?;
		// A registration body needs an active local identity to sign for.
		self.wallet
			.session_state()
			.current()
			.and_then(|session| session.identity_account_id)
			.ok_or_else(|| error("no active local identity"))?;
		let signed_at = identity::registration_timestamp(&self.services.chain, genesis)
			.await
			.map_err(error)?;
		let signing_contexts = self.registration_signing_contexts(genesis).await?;
		// Loaded before the activation lock: the browser fetches `verifiable`
		// on first use, and its operations below are then synchronous.
		let vrf = crate::runtime::vrf::load().await.map_err(error)?;
		self.check_identity_activation(activation_id)?;
		let entropy = self.wallet.root_entropy().map_err(error)?;
		let vrf_entropy = derive_lite_person_ring_vrf_entropy(&entropy, self.network_suffix());
		let registration = attestation::build_lite_registration_with(
			&entropy,
			self.network_suffix(),
			verifier,
			username_base,
			None,
			signed_at,
			signing_contexts,
			vrf.member(&vrf_entropy).map_err(error)?,
			|message| {
				let signature = vrf.sign(&vrf_entropy, message).map_err(|err| err.to_string())?;
				<[u8; 64]>::try_from(signature.as_slice())
					.map_err(|_| format!("ring-VRF signature is {} bytes", signature.len()))
			},
		)
		.map_err(error)?;
		Ok(registration.request_body(username_base, None, signed_at).to_string())
	}

	/// Install chain-verified Lite metadata only if the local activation remains current.
	pub async fn refresh_local_identity(
		&self,
		activation_id: &str,
	) -> Result<LocalIdentity, GenericError> {
		self.check_identity_activation(activation_id)?;
		let genesis = self.identity_asset_hub().await?;
		self.check_identity_activation(activation_id)?;
		let account = self
			.wallet
			.session_state()
			.current()
			.and_then(|session| session.identity_account_id)
			.ok_or_else(|| error("no active local identity"))?;
		let resolved = identity::lookup_local_identity(&self.services.chain, genesis, account)
			.await
			.map_err(error)?;
		self.check_identity_activation(activation_id)?;
		let mut session = self
			.wallet
			.session_state()
			.current()
			.ok_or_else(|| error("no active local identity"))?;
		if session.identity_account_id != Some(account) {
			return Err(error("local identity account changed"));
		}
		let session_before = session.clone();
		(session.lite_username, session.full_username) =
			resolved.map(dotns_gateway::DotnsIdentity::into_usernames).unwrap_or_default();
		let result = LocalIdentity {
			identity_account_id: format!("0x{}", hex::encode(account)),
			lite_username: session.lite_username.clone(),
		};
		let ui_info = connected_session_ui_info(&session);
		// Only the wallet the lookup began under takes the names.
		if !self.wallet.session_state().replace_session_if_current(&session_before, session) {
			return Err(error("local identity activation changed"));
		}
		self.auth_state.connected(&ui_info);
		Ok(result)
	}
}
