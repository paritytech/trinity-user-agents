use super::SigningHost;
use crate::runtime::{
	authority::{AccountHolder, AuthorityError},
	connected_session_ui_info,
	identity::lookup_session_identity,
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
		let permissions = self.services.permissions.invalidate_pending(None).await;
		let (session, selected) = self.change_activation(|| {
			let session = self.wallet.install(activation);
			let selected = self.wallet.current_session().expect("just installed wallet session");
			(session, selected)
		});
		drop(permissions);
		self.auth_state.connected(&connected_session_ui_info(&session));

		let services = self.services.clone();
		let auth_state = self.auth_state.clone();
		let wallet = self.wallet.clone();
		(self.services.spawner)(Box::pin(async move {
			let identity = match lookup_session_identity(
				&services.chain,
				services.asset_hub_chain_genesis_hash().unwrap_or_default(),
				&session,
			)
			.await
			{
				Ok(Some(identity)) => identity,
				Ok(None) => return,
				Err(reason) => {
					tracing::warn!(
						%reason,
						"dotNS username resolution failed; the session stays without one"
					);
					return;
				},
			};
			let (lite_username, full_username) = identity.into_usernames();
			let Ok(named) =
				wallet.install_resolved_identity(&selected, lite_username, full_username)
			else {
				return;
			};
			auth_state.connected(&connected_session_ui_info(&named));
		}));
		Ok(())
	}
}
