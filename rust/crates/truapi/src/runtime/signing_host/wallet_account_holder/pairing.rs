use super::{WalletAccountHolder, product_authority_error};
use crate::host_logic::session::SsoSessionInfo;
use crate::host_logic::sso::pairing::{
    bootstrap_topic, encrypt_v2_handshake_response, establish_responder_session_info, v2,
};
use crate::host_logic::statement_store::build_signed_statement;
use crate::runtime::AccountHolder;
use crate::runtime::authority::{AuthorityError, AuthoritySession};
use crate::runtime::sso_responder_service::PairedSsoPeer;
use crate::runtime::sso_remote::fresh_statement_expiry;
use parity_scale_codec::Encode;

impl WalletAccountHolder {
    /// Verify both keys of an externally owned SSO transport.
    pub fn require_sso_identity(
        &self,
        session: &AuthoritySession,
        statement_public_key: [u8; 32],
        encryption_public_key: [u8; 32],
    ) -> Result<(), AuthorityError> {
        self.with_keys(session, |keys| {
            let (identity, _) = keys.responder_identity().map_err(product_authority_error)?;
            if identity.statement_public_key != statement_public_key
                || identity.encryption_public_key != encryption_public_key
            {
                return Err(AuthorityError::Unavailable {
                    reason: "SSO transport identity does not match the active wallet".to_string(),
                });
            }
            Ok(())
        })
    }

    /// Derive transport material for a peer under the selected activation.
    pub fn sso_session(
        &self,
        session: &AuthoritySession,
        peer: PairedSsoPeer,
    ) -> Result<SsoSessionInfo, AuthorityError> {
        self.with_keys(session, |keys| {
            let (identity, _) = keys.responder_identity().map_err(product_authority_error)?;
            establish_responder_session_info(
                &identity,
                peer.statement_account_id,
                peer.encryption_public_key,
            )
            .map_err(|reason| AuthorityError::Unknown { reason })
        })
    }

    /// Encrypt delegated derivation material before it leaves the wallet.
    pub fn pairing_answer(
        &self,
        session: &AuthoritySession,
        peer: PairedSsoPeer,
        device_enc_pub_key: [u8; 32],
    ) -> Result<(SsoSessionInfo, Vec<u8>), AuthorityError> {
        self.with_keys(session, |keys| {
            let (identity, chat_private_key) =
                keys.responder_identity().map_err(product_authority_error)?;
            let transport = establish_responder_session_info(
                &identity,
                peer.statement_account_id,
                peer.encryption_public_key,
            )
            .map_err(|reason| AuthorityError::Unknown { reason })?;
            let response = v2::EncryptedResponse::Success(Box::new(v2::Success {
                identity_account_id: identity.statement_public_key,
                root_account_id: session.public_key,
                identity_chat_private_key: chat_private_key,
                sso_enc_pub_key: identity.encryption_public_key,
                device_enc_pub_key,
                root_entropy_source: keys.root_entropy_source(),
            }));
            let statement = prepare_handshake_answer(&transport, peer, &response)?;
            Ok((transport, statement))
        })
    }

    /// Sign a pairing notice only while its original wallet is selected.
    pub fn pairing_notice(
        &self,
        session: &AuthoritySession,
        peer: PairedSsoPeer,
        response: &v2::EncryptedResponse,
    ) -> Result<Vec<u8>, AuthorityError> {
        let transport = self.sso_session(session, peer)?;
        self.require_current_session(session)?;
        prepare_handshake_answer(&transport, peer, response)
    }
}

fn prepare_handshake_answer(
    session: &SsoSessionInfo,
    peer: PairedSsoPeer,
    response: &v2::EncryptedResponse,
) -> Result<Vec<u8>, AuthorityError> {
    let handshake = encrypt_v2_handshake_response(peer.encryption_public_key, response)
        .map_err(|reason| AuthorityError::Unknown { reason })?;
    let topic = bootstrap_topic(peer.statement_account_id, peer.encryption_public_key);
    build_signed_statement(
        session,
        topic,
        topic,
        handshake.encode(),
        fresh_statement_expiry(),
    )
    .map_err(|reason| AuthorityError::Unknown { reason })
}
