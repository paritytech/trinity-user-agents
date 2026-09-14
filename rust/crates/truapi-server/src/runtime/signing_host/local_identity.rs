//! Wallet-local identity proofs and authoritative dotNS metadata refresh.

use super::SigningHost;
use crate::host_logic::{attestation, dotns_gateway, features};
use crate::runtime::{connected_session_ui_info, identity};
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
    GenericError {
        reason: reason.to_string(),
    }
}

impl SigningHost {
    /// Capture the current UID account and local activation generation atomically.
    pub(crate) fn local_identity_context(&self) -> Result<LocalIdentityContext, GenericError> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        let account = self
            .session_state
            .current()
            .and_then(|session| session.identity_account_id)
            .ok_or_else(|| error("no active local identity"))?;
        Ok(LocalIdentityContext {
            activation_id: state.activation_generation.to_string(),
            identity_account_id: format!("0x{}", hex::encode(account)),
        })
    }

    fn check_identity_activation(&self, activation_id: &str) -> Result<(), GenericError> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        if activation_id.parse::<u64>().ok() != Some(state.activation_generation)
            || self.session_state.current().is_none()
        {
            return Err(error("local identity activation changed"));
        }
        Ok(())
    }

    async fn identity_asset_hub(&self) -> Result<[u8; 32], GenericError> {
        let chains = features::supported_chains(self.platform.as_ref()).await?;
        features::genesis_for(&chains, ChainIdentifier::AssetHub)
            .ok_or_else(|| error("Asset Hub is not configured"))
    }

    /// Sign the backend token challenge without releasing UID secret material.
    pub(crate) fn local_identity_auth_proof(
        &self,
        activation_id: &str,
        challenge: &[u8],
    ) -> Result<Vec<u8>, GenericError> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        if activation_id.parse::<u64>().ok() != Some(state.activation_generation) {
            return Err(error("local identity activation changed"));
        }
        let entropy = self.root_entropy().map_err(error)?;
        let (client_id, proof) =
            attestation::sign_backend_challenge(&entropy, self.network_suffix(), challenge, b"{}")
                .map_err(error)?;
        let mut bytes = Vec::with_capacity(96);
        bytes.extend_from_slice(&client_id);
        bytes.extend_from_slice(&proof);
        Ok(bytes)
    }

    /// Build shared registration JSON with proofs timestamped at the configured Asset Hub.
    pub(crate) async fn local_lite_registration_body(
        &self,
        activation_id: &str,
        username_base: &str,
        verifier: [u8; 32],
    ) -> Result<String, GenericError> {
        if !dotns_gateway::is_registrable_full_label(username_base)
            || username_base.len() + 3 > dotns_gateway::MAX_BASE_LABEL_LEN
        {
            return Err(error(
                "Lite username base must contain 6 to 29 lowercase ASCII letters",
            ));
        }
        self.check_identity_activation(activation_id)?;
        let genesis = self.identity_asset_hub().await?;
        self.check_identity_activation(activation_id)?;
        let account = self
            .session_state
            .current()
            .and_then(|session| session.identity_account_id)
            .ok_or_else(|| error("no active local identity"))?;
        let signed_at = identity::registration_timestamp(&self.services.chain, genesis, account)
            .await
            .map_err(error)?;
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        if activation_id.parse::<u64>().ok() != Some(state.activation_generation) {
            return Err(error("local identity activation changed"));
        }
        let entropy = self.root_entropy().map_err(error)?;
        let registration = attestation::build_lite_registration(
            &entropy,
            self.network_suffix(),
            verifier,
            username_base,
            None,
            signed_at,
        )
        .map_err(error)?;
        Ok(registration
            .request_body(username_base, None, signed_at)
            .to_string())
    }

    /// Install chain-verified Lite metadata only if the local activation remains current.
    pub(crate) async fn refresh_local_identity(
        &self,
        activation_id: &str,
    ) -> Result<LocalIdentity, GenericError> {
        self.check_identity_activation(activation_id)?;
        let genesis = self.identity_asset_hub().await?;
        self.check_identity_activation(activation_id)?;
        let account = self
            .session_state
            .current()
            .and_then(|session| session.identity_account_id)
            .ok_or_else(|| error("no active local identity"))?;
        let resolved = identity::lookup_local_identity(&self.services.chain, genesis, account)
            .await
            .map_err(error)?;
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        if activation_id.parse::<u64>().ok() != Some(state.activation_generation) {
            return Err(error("local identity activation changed"));
        }
        let mut session = self
            .session_state
            .current()
            .ok_or_else(|| error("no active local identity"))?;
        if session.identity_account_id != Some(account) {
            return Err(error("local identity account changed"));
        }
        session.lite_username = resolved.lite_username;
        session.full_username = resolved.full_username;
        let result = LocalIdentity {
            identity_account_id: format!("0x{}", hex::encode(account)),
            lite_username: session.lite_username.clone(),
        };
        let ui_info = connected_session_ui_info(&session);
        self.session_state.set_session(session);
        drop(state);
        self.auth_state.connected(&ui_info);
        Ok(result)
    }
}
