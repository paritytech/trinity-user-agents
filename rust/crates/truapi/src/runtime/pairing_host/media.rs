//! Media uses the authenticated SSO channel only for account certification.
//! Transport proofs use the already-owned session statement key, never a new allowance.

use parity_scale_codec::Encode;
use truapi::CallContext;
use crate::platform::normalize_product_identifier;

use super::PairingHost;
use super::sso_channel::remote_authority_error;
use crate::host_logic::media_protocol::{
    MAX_PRODUCT_ID_BYTES, MediaIdentity, UnsignedAdvertisement, validate_unsigned_advertisement,
};
use crate::host_logic::product_account::{derive_product_public_key, index_bytes};
use crate::host_internal::sso_messages::{
    MediaEndpointCertificationError, MediaEndpointCertificationRequest,
};
use crate::host_logic::statement_store::sign_statement_fields;
use crate::unix_time::current_unix_secs;
use crate::runtime::authority::{AuthorityError, AuthoritySession, media_statement_fields};

impl PairingHost {
    pub(super) async fn certify_paired_media_endpoint(
        &self,
        cx: &CallContext,
        session: &AuthoritySession,
        unsigned: UnsignedAdvertisement,
    ) -> Result<[u8; 64], AuthorityError> {
        let (current, epoch) = {
            let lifecycle = self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
            (self.current_private_session(session)?, lifecycle.epoch)
        };
        if unsigned.product_id.is_empty()
            || unsigned.product_id.len() > MAX_PRODUCT_ID_BYTES
            || unsigned.network != self.statement_store.genesis_hash()
            || normalize_product_identifier(&unsigned.product_id)
                .map_err(|_| AuthorityError::Rejected)? != unsigned.product_id
        {
            return Err(AuthorityError::Rejected);
        }
        let subtree = self.remote_product_subtree_public_key(
            cx,
            &current,
            unsigned.product_id.clone(),
        ).await?;
        let expected = MediaIdentity {
            network: self.statement_store.genesis_hash(),
            product_id: unsigned.product_id.clone(),
            account: derive_product_public_key(subtree, index_bytes(0))
                .map_err(|_| AuthorityError::Rejected)?,
        };
        validate_unsigned_advertisement(&unsigned, &expected, current_unix_secs())
            .map_err(|_| AuthorityError::Rejected)?;
        self.current_private_session(session)?;
        if !self.is_session_lifecycle_current(epoch) {
            return Err(AuthorityError::Disconnected);
        }
        let signature = self.call(
            cx,
            &current,
            MediaEndpointCertificationRequest {
                product_id: expected.product_id,
                unsigned_advertisement: unsigned.encode(),
            },
        ).await.map_err(remote_authority_error)?
            .map_err(|error| match error {
                MediaEndpointCertificationError::Disconnected => AuthorityError::Disconnected,
                MediaEndpointCertificationError::Rejected => AuthorityError::Rejected,
                MediaEndpointCertificationError::Unavailable => AuthorityError::Unavailable {
                    reason: "Media certification unavailable".to_string(),
                },
            })?;
        // Authenticate the answer and fence delayed replies against logout or
        // replacement, including reactivation with the same persisted SSO ids.
        let lifecycle = self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
        if lifecycle.epoch != epoch {
            return Err(AuthorityError::Disconnected);
        }
        self.current_private_session(session)?;
        unsigned.authenticate(signature, current_unix_secs())
            .map_err(|_| AuthorityError::Rejected)?;
        Ok(signature)
    }

    pub(super) fn sign_paired_media_statement(
        &self,
        session: &AuthoritySession,
        payload: Vec<u8>,
        topics: Vec<[u8; 32]>,
        expires_at: u64,
    ) -> Result<Vec<u8>, AuthorityError> {
        let fields = media_statement_fields(payload, topics, expires_at)?;
        let _lifecycle = self.session_lifecycle.lock().expect("session lifecycle mutex poisoned");
        let current = self.current_private_session(session)?;
        let sso = current.sso.as_ref().ok_or(AuthorityError::Disconnected)?;
        sign_statement_fields(sso.ss_secret, sso.ss_public_key, fields)
            .map(|fields| fields.encode())
            .map_err(|_| AuthorityError::Unavailable {
                reason: "Media statement signing unavailable".to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use super::*;
    use crate::host_logic::statement_store::decode_verified_statement_data;
    use crate::runtime::authority::ProductAuthority;
    use crate::runtime::services::RuntimeServices;
    use crate::test_support::{StubPlatform, runtime_config, sso_session_info, test_spawner};

    #[test]
    fn media_transport_uses_the_existing_paired_key_and_stops_on_disconnect() {
        let (config, _) = runtime_config("myapp.dot");
        let services = RuntimeServices::new(
            Arc::new(StubPlatform::default()),
            config.host.host_info.clone(),
            config.people_chain_genesis_hash,
            config.bulletin_chain_genesis_hash,
            config.asset_hub_chain_genesis_hash,
            test_spawner(),
        );
        let authority = PairingHost::new(services, config);
        let private_session = sso_session_info();
        let expected_signer = private_session.sso.as_ref().unwrap().ss_public_key;
        futures::executor::block_on(authority.set_connected_session_for_tests(private_session));
        let session = authority.current_session().unwrap();
        let expires_at = current_unix_secs() + 60;
        let bytes = authority.sign_media_statement(
            &session, vec![3, 2, 1], vec![[9; 32]], expires_at,
        ).unwrap();
        let verified = decode_verified_statement_data(&bytes, Some(expected_signer)).unwrap();
        assert_eq!(verified.data, vec![3, 2, 1]);
        assert_eq!(verified.expiry, Some(expires_at << 32));
        futures::executor::block_on(authority.disconnect());
        assert_eq!(
            authority.sign_media_statement(&session, vec![1], vec![[9; 32]], expires_at),
            Err(AuthorityError::Disconnected),
        );
    }
}
