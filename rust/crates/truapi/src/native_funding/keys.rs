//! The session keys the native route moves funds through.
//!
//! Each session gets its own key per direction, hard-derived from the user's
//! root entropy and never stored: `//funding//<installation>//<intent>` takes
//! a top-up's deposit on Asset Hub and holds its CASH on People, and
//! `//funding//<installation>//<intent>//out` holds a withdrawal's CASH. Every
//! junction is hard, so the secret a top-up hands the host reveals nothing
//! about any other key. The key is recoverable from the seed with the session
//! id and the installation.

use std::sync::Weak;

use schnorrkel::Keypair;
use truapi::latest::FundingDirection;
use zeroize::Zeroizing;

use crate::{
	host_internal::extrinsic::Sr25519Signer,
	host_logic::product_account::{ProductAccountError, derive_sr25519_hard_path},
	installation::InstallationId,
	runtime::AuthorityError,
};

/// Where a session key sits in the derivation tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionKeySlot {
	/// The installation that serves the session.
	pub installation: InstallationId,
	/// The funding session.
	pub intent: String,
	/// The session's direction.
	pub direction: FundingDirection,
}

impl SessionKeySlot {
	/// The hard junctions of its path. The installation is written as `0x`
	/// and lowercase hex, as NFT item keys write it.
	pub fn junctions(&self) -> Vec<String> {
		let mut junctions = vec![
			"funding".to_string(),
			format!("0x{}", hex::encode(self.installation.0)),
			self.intent.clone(),
		];
		if self.direction == FundingDirection::Out {
			junctions.push("out".to_string());
		}
		junctions
	}
}

/// A session key: its public key, a signer for it, and its 64-byte secret,
/// wiped when dropped.
pub struct SessionKey {
	/// The public key, which is also its account id.
	pub public: [u8; 32],
	/// Signs as the key.
	pub signer: Sr25519Signer,
	/// The secret, as `payment.topUp` takes a private-key source.
	pub secret: Zeroizing<[u8; 64]>,
}

impl SessionKey {
	fn from_keypair(keypair: &Keypair) -> Self {
		Self {
			public: keypair.public.to_bytes(),
			signer: Sr25519Signer::from_keypair(keypair),
			secret: Zeroizing::new(keypair.secret.to_bytes()),
		}
	}
}

/// Derives session keys. The root entropy never leaves the implementor.
pub trait SessionKeys: Send + Sync {
	/// The key at `slot`. Fails without a session.
	fn session_key(&self, slot: &SessionKeySlot) -> Result<SessionKey, AuthorityError>;
}

/// For a deriver that owns the native route, which must not keep it alive
/// in turn.
impl<Keys: SessionKeys> SessionKeys for Weak<Keys> {
	fn session_key(&self, slot: &SessionKeySlot) -> Result<SessionKey, AuthorityError> {
		let Some(keys) = self.upgrade() else {
			let reason = "the session key deriver was dropped".to_string();
			return Err(AuthorityError::Unavailable { reason });
		};
		keys.session_key(slot)
	}
}

/// The key at `slot`, derived from the root of `entropy`.
pub fn session_key_from_entropy(
	entropy: &[u8],
	slot: &SessionKeySlot,
) -> Result<SessionKey, ProductAccountError> {
	let junctions = slot.junctions();
	let junctions: Vec<&str> = junctions.iter().map(String::as_str).collect();
	Ok(SessionKey::from_keypair(&derive_sr25519_hard_path(entropy, &junctions)?))
}

#[cfg(test)]
mod tests {
	use super::*;

	const ENTROPY: [u8; 16] = [7; 16];

	fn slot(intent: &str, direction: FundingDirection) -> SessionKeySlot {
		SessionKeySlot { installation: InstallationId([0xab; 32]), intent: intent.into(), direction }
	}

	#[test]
	fn the_path_names_the_installation_the_session_and_an_outbound_leg() {
		let installation = format!("0x{}", "ab".repeat(32));
		assert_eq!(
			slot("fs_1", FundingDirection::In).junctions(),
			["funding", installation.as_str(), "fs_1"]
		);
		assert_eq!(
			slot("fs_1", FundingDirection::Out).junctions(),
			["funding", installation.as_str(), "fs_1", "out"]
		);
	}

	#[test]
	fn keys_differ_by_session_and_direction_and_derive_again_the_same() {
		let key = |intent, direction| {
			session_key_from_entropy(&ENTROPY, &slot(intent, direction)).expect("derives").public
		};

		assert_eq!(key("fs_1", FundingDirection::In), key("fs_1", FundingDirection::In));
		assert_ne!(key("fs_1", FundingDirection::In), key("fs_2", FundingDirection::In));
		assert_ne!(key("fs_1", FundingDirection::In), key("fs_1", FundingDirection::Out));
	}

	#[test]
	fn the_secret_signs_for_the_public_key() {
		let key = session_key_from_entropy(&ENTROPY, &slot("fs_1", FundingDirection::In))
			.expect("derives");
		let secret = schnorrkel::SecretKey::from_bytes(key.secret.as_ref()).expect("a secret");
		assert_eq!(secret.to_public().to_bytes(), key.public);
	}
}
