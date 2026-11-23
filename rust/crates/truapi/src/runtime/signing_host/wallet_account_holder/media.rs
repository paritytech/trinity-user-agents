//! Private Media signing under the active wallet's lifecycle fence.

use parity_scale_codec::Encode;
use schnorrkel::Keypair;
use truapi::latest;

use super::{WalletAccountHolder, WalletKeys};
use crate::{
	host_logic::{
		media_protocol::{
			ACCOUNT_SIGNING_CONTEXT, MAX_PRODUCT_ID_BYTES, MAX_UNSIGNED_ADVERTISEMENT_BYTES,
			MediaIdentity, UnsignedAdvertisement, decode_unsigned_advertisement,
			validate_unsigned_advertisement,
		},
		statement_store::sign_statement_fields,
	},
	platform::ProductId,
	runtime::authority::{AuthorityError, AuthoritySession, media_statement_fields},
	unix_time::current_unix_secs,
};

impl WalletAccountHolder {
	fn media_account_keypair(
		&self,
		keys: &WalletKeys,
		product_id: &str,
	) -> Result<(MediaIdentity, Keypair), AuthorityError> {
		if product_id.is_empty() || product_id.len() > MAX_PRODUCT_ID_BYTES {
			return Err(AuthorityError::Rejected);
		}
		let canonical = product_id.parse::<ProductId>().map_err(|_| AuthorityError::Rejected)?;
		if canonical.as_str() != product_id {
			return Err(AuthorityError::Rejected);
		}
		let account = latest::ProductAccountId {
			dot_ns_identifier: canonical,
			derivation_index: latest::DerivationIndex::Index(0),
		};
		let keypair = keys.product_keypair(&account)?;
		let identity = MediaIdentity {
			network: self.services.statement_store.genesis_hash(),
			product_id: product_id.to_owned(),
			account: keypair.public.to_bytes(),
		};
		Ok((identity, keypair))
	}

	pub(crate) fn certify_local_media_endpoint(
		&self,
		session: &AuthoritySession,
		unsigned: UnsignedAdvertisement,
	) -> Result<[u8; 64], AuthorityError> {
		self.with_keys(session, |keys| {
			let (expected, keypair) = self.media_account_keypair(keys, &unsigned.product_id)?;
			validate_unsigned_advertisement(&unsigned, &expected, current_unix_secs())
				.map_err(|_| AuthorityError::Rejected)?;
			Ok(sign_advertisement(&keypair, &unsigned))
		})
	}

	/// Bind the outer SSO label to the canonical, bounded Index(0) payload.
	pub(crate) fn certify_encoded_media_endpoint(
		&self,
		session: &AuthoritySession,
		product_id: &str,
		payload: &[u8],
	) -> Result<[u8; 64], AuthorityError> {
		if payload.len() > MAX_UNSIGNED_ADVERTISEMENT_BYTES {
			return Err(AuthorityError::Rejected);
		}
		self.with_keys(session, |keys| {
			let (expected, keypair) = self.media_account_keypair(keys, product_id)?;
			let unsigned = decode_unsigned_advertisement(payload, &expected, current_unix_secs())
				.map_err(|_| AuthorityError::Rejected)?;
			Ok(sign_advertisement(&keypair, &unsigned))
		})
	}

	pub(crate) fn sign_local_media_statement(
		&self,
		session: &AuthoritySession,
		payload: Vec<u8>,
		topics: Vec<[u8; 32]>,
		expires_at: u64,
	) -> Result<Vec<u8>, AuthorityError> {
		let fields = media_statement_fields(payload, topics, expires_at)?;
		self.with_keys(session, |keys| {
			let keypair = keys.identity_keypair()?;
			sign_statement_fields(keypair.secret.to_bytes(), keypair.public.to_bytes(), fields)
				.map(|fields| fields.encode())
				.map_err(|_| AuthorityError::Unavailable {
					reason: "Media statement signing unavailable".to_string(),
				})
		})
	}
}

fn sign_advertisement(keypair: &Keypair, unsigned: &UnsignedAdvertisement) -> [u8; 64] {
	keypair
		.secret
		.sign_simple(ACCOUNT_SIGNING_CONTEXT, &unsigned.account_signing_input(), &keypair.public)
		.to_bytes()
}
