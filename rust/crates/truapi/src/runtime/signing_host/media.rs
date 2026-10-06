//! Private endpoint certification and existing-identity transport signing.

use std::sync::MutexGuard;

use parity_scale_codec::Encode;
use schnorrkel::Keypair;
use truapi::v01;
use crate::platform::normalize_product_identifier;

use super::{LocalGrantState, SigningHost, local_session_validation_id};
use crate::host_logic::media_protocol::{
    ACCOUNT_SIGNING_CONTEXT, MAX_PRODUCT_ID_BYTES, MAX_UNSIGNED_ADVERTISEMENT_BYTES,
    MediaIdentity, UnsignedAdvertisement, decode_unsigned_advertisement,
    validate_unsigned_advertisement,
};
use crate::host_logic::statement_store::sign_statement_fields;
use crate::unix_time::current_unix_secs;
use crate::runtime::authority::{AuthorityError, AuthoritySession, media_statement_fields};

impl SigningHost {
    /// Hold the activation fence through key derivation and signing. A concurrent
    /// logout/replacement cannot turn a checked snapshot into a different wallet.
    fn media_session_guard(
        &self,
        session: &AuthoritySession,
    ) -> Result<MutexGuard<'_, LocalGrantState>, AuthorityError> {
        let state = self
            .local_grants
            .lock()
            .expect("local AutoSigning grant mutex poisoned");
        let current = self.session_state.current().ok_or(AuthorityError::Disconnected)?;
        if local_session_validation_id(&current, state.activation_generation)
            != session.validation_id
        {
            return Err(AuthorityError::Disconnected);
        }
        Ok(state)
    }

    fn media_account_keypair(
        &self,
        product_id: &str,
    ) -> Result<(MediaIdentity, Keypair), AuthorityError> {
        if product_id.is_empty() || product_id.len() > MAX_PRODUCT_ID_BYTES {
            return Err(AuthorityError::Rejected);
        }
        let canonical = normalize_product_identifier(product_id)
            .map_err(|_| AuthorityError::Rejected)?;
        if canonical != product_id {
            return Err(AuthorityError::Rejected);
        }
        let account = v01::ProductAccountId {
            dot_ns_identifier: canonical,
            derivation_index: v01::DerivationIndex::Index(0),
        };
        let keypair = self.product_keypair(&account)?;
        let identity = MediaIdentity {
            network: self.services.statement_store.genesis_hash(),
            product_id: account.dot_ns_identifier,
            account: keypair.public.to_bytes(),
        };
        Ok((identity, keypair))
    }

    pub(super) fn certify_local_media_endpoint(
        &self,
        session: &AuthoritySession,
        unsigned: UnsignedAdvertisement,
    ) -> Result<[u8; 64], AuthorityError> {
        let _guard = self.media_session_guard(session)?;
        let (expected, keypair) = self.media_account_keypair(&unsigned.product_id)?;
        validate_unsigned_advertisement(&unsigned, &expected, current_unix_secs())
            .map_err(|_| AuthorityError::Rejected)?;
        Ok(sign_advertisement(&keypair, &unsigned))
    }

    /// The SSO product label is not a signer: derive Index(0) independently and
    /// require the bounded canonical payload to bind to that exact identity.
    pub(super) fn certify_encoded_media_endpoint(
        &self,
        session: &AuthoritySession,
        product_id: &str,
        payload: &[u8],
    ) -> Result<[u8; 64], AuthorityError> {
        let _guard = self.media_session_guard(session)?;
        if payload.len() > MAX_UNSIGNED_ADVERTISEMENT_BYTES {
            return Err(AuthorityError::Rejected);
        }
        let (expected, keypair) = self.media_account_keypair(product_id)?;
        let unsigned = decode_unsigned_advertisement(payload, &expected, current_unix_secs())
            .map_err(|_| AuthorityError::Rejected)?;
        Ok(sign_advertisement(&keypair, &unsigned))
    }

    pub(super) fn sign_local_media_statement(
        &self,
        session: &AuthoritySession,
        payload: Vec<u8>,
        topics: Vec<[u8; 32]>,
        expires_at: u64,
    ) -> Result<Vec<u8>, AuthorityError> {
        let fields = media_statement_fields(payload, topics, expires_at)?;
        let _guard = self.media_session_guard(session)?;
        let keypair = self.identity_keypair()?;
        sign_statement_fields(keypair.secret.to_bytes(), keypair.public.to_bytes(), fields)
            .map(|fields| fields.encode())
            .map_err(|_| AuthorityError::Unavailable {
                reason: "Media statement signing unavailable".to_string(),
            })
    }
}

fn sign_advertisement(keypair: &Keypair, unsigned: &UnsignedAdvertisement) -> [u8; 64] {
    keypair
        .secret
        .sign_simple(ACCOUNT_SIGNING_CONTEXT, &unsigned.account_signing_input(), &keypair.public)
        .to_bytes()
}
