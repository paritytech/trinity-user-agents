use super::SigningHost;
use crate::{
	host_logic::{root_entropy::RootEntropy, session::SessionInfo},
	runtime::{
		AccountHolder,
		authority::AuthorityError,
		connected_session_ui_info, device_enc_public_key,
		identity::{LookupOutcome, lookup_session_identity},
	},
};

/// Activate the wallet from entropy supplied by the embedding host.
#[async_trait::async_trait]
pub trait LocalActivation: Send + Sync {
	/// Activate a local session from the root entropy, deriving the root
	/// public key and marking the session connected.
	async fn activate_local_session(&self, entropy: RootEntropy) -> Result<(), AuthorityError>;

	/// Activate a local session under the host's known `lite_username` until
	/// the dotNS record on Asset Hub answers. A record with a name replaces it.
	async fn activate_local_session_with_identity(
		&self,
		entropy: RootEntropy,
		lite_username: Option<String>,
	) -> Result<(), AuthorityError>;
}

#[async_trait::async_trait]
impl LocalActivation for SigningHost {
	async fn activate_local_session(&self, entropy: RootEntropy) -> Result<(), AuthorityError> {
		self.activate_local_session_with_identity(entropy, None).await
	}

	async fn activate_local_session_with_identity(
		&self,
		entropy: RootEntropy,
		lite_username: Option<String>,
	) -> Result<(), AuthorityError> {
		let device_enc_public_key =
			self.wallet.device_encryption_secret().await?.map(device_enc_public_key);
		let activation = self.wallet.prepare_activation(entropy, lite_username)?;
		let session = self.accounts.change_activation(|| self.wallet.install(activation));
		self.auth_state
			.connected(&connected_session_ui_info(&session, device_enc_public_key));

		let services = self.services.clone();
		let session_state = self.wallet.session_state();
		let auth_state = self.auth_state.clone();
		self.services.spawner.spawn(async move {
			let LookupOutcome::Found(identity) = lookup_session_identity(
				&services.dotns_reads,
				&services.chain,
				services.asset_hub_chain_genesis_hash(),
				&session,
			)
			.await
			else {
				return;
			};
			let (lite_username, full_username) = identity.into_usernames();
			let named = SessionInfo { lite_username, full_username, ..session.clone() };
			if session_state.replace_session_if_current(&session, named.clone()) {
				auth_state.connected(&connected_session_ui_info(&named, device_enc_public_key));
			}
		});
		Ok(())
	}
}
