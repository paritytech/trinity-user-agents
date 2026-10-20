use super::{SigningHost, product_authority_error};
use crate::{
	host_logic::{
		product_account::{derive_identity_keypair, derive_root_keypair_from_entropy},
		session::SessionInfo,
		sso::pairing::derive_identity_chat_private_key,
	},
	runtime::{
		authority::AuthorityError, connected_session_ui_info, identity::lookup_session_identity,
	},
};

use zeroize::Zeroizing;

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
		let secret = Zeroizing::new(secret);
		let root = derive_root_keypair_from_entropy(&secret).map_err(product_authority_error)?;
		let public_key = root.public.to_bytes();
		let identity_account_id = derive_identity_keypair(&secret, self.network_suffix())
			.map_err(product_authority_error)?
			.public
			.to_bytes();
		let identity_chat_private_key = derive_identity_chat_private_key(&secret);
		let session = SessionInfo {
			public_key,
			sso: None,
			root_entropy_source: None,
			identity_account_id: Some(identity_account_id),
			identity_chat_private_key: Some(identity_chat_private_key),
			// A local session has no answering remote device to address.
			device_enc_public_key: None,
			lite_username,
			full_username: None,
		};
		let ui_info = connected_session_ui_info(&session);
		let switched = self
			.session_state
			.current()
			.is_some_and(|previous| previous.public_key != public_key);
		if switched {
			self.services.drop_auth_tokens_after_session().await;
		}
		self.install_local_session(secret, session.clone());
		self.auth_state.connected(&ui_info);

		let services = self.services.clone();
		let session_state = self.session_state.clone();
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
