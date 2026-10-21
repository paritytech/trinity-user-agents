use super::SigningHost;
use crate::{
	host_logic::session::SessionInfo,
	runtime::{
		authority::AuthorityError, connected_session_ui_info, identity::lookup_session_identity,
	},
};

/// Activate the wallet from entropy supplied by the embedding host.
#[async_trait::async_trait]
pub trait LocalActivation: Send + Sync {
	/// Activate a local session from raw BIP-39 entropy, deriving the root
	/// public key and marking the session connected.
	async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError>;

	/// Activate a local session under the host's known `lite_username` until
	/// the dotNS record on Asset Hub answers. A record with a name replaces it.
	async fn activate_local_session_with_identity(
		&self,
		secret: Vec<u8>,
		lite_username: Option<String>,
	) -> Result<(), AuthorityError>;
}

#[async_trait::async_trait]
impl LocalActivation for SigningHost {
	async fn activate_local_session(&self, secret: Vec<u8>) -> Result<(), AuthorityError> {
		self.activate_local_session_with_identity(secret, None).await
	}

	async fn activate_local_session_with_identity(
		&self,
		secret: Vec<u8>,
		lite_username: Option<String>,
	) -> Result<(), AuthorityError> {
		let activation = self.wallet.prepare_activation(secret, lite_username)?;
		let session = self.accounts.change_activation(|| self.wallet.install(activation));
		self.auth_state.connected(&connected_session_ui_info(&session));

		let services = self.services.clone();
		let session_state = self.wallet.session_state();
		let auth_state = self.auth_state.clone();
		(self.services.spawner)(Box::pin(async move {
			let Some(identity) = lookup_session_identity(
				&services.chain,
				services.asset_hub_chain_genesis_hash().unwrap_or_default(),
				&session,
			)
			.await
			else {
				return;
			};
			let (lite_username, full_username) = identity.into_usernames();
			let named = SessionInfo { lite_username, full_username, ..session.clone() };
			if session_state.replace_session_if_current(&session, named.clone()) {
				auth_state.connected(&connected_session_ui_info(&named));
			}
		}));
		Ok(())
	}
}
