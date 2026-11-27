use schnorrkel::{PublicKey, Signature};

use super::*;
use crate::{
	host_logic::{
		sso::pairing::{ResponderIdentity, establish_responder_session_info},
		x25519_seal::X25519PrivateKey,
	},
	test_support::{sso_host_and_responder_sessions, sso_session_info},
};

fn sso() -> SsoSessionInfo {
	sso_session_info().sso.expect("fixture carries an SSO session")
}

/// Whether `request`'s signature is its new statement key's over the change
/// of the session whose key is `replaces`.
fn signed_by_new_key(request: &RotateSessionRequest, replaces: [u8; 32]) -> bool {
	let payload = RotateSessionRequest::signed_payload(
		replaces,
		request.statement_account_id,
		request.encryption_public_key,
	);
	let signer = PublicKey::from_bytes(&request.statement_account_id).unwrap();
	let signature = Signature::from_bytes(&request.signature).unwrap();
	signer.verify_simple(SR25519_SIGNING_CONTEXT, &payload, &signature).is_ok()
}

#[test]
fn a_key_change_is_signed_by_its_new_key_for_the_session_it_replaces_only() {
	let session = sso();
	let request = PendingSessionRotation::start(&session, "rotate-1".to_string())
		.unwrap()
		.request();

	assert_eq!(
		[
			signed_by_new_key(&request, session.ss_public_key),
			signed_by_new_key(&request, [0xEE; 32]),
		],
		[true, false],
		"a signature that did not name the replaced key could move another session"
	);
}

#[test]
fn the_changed_session_meets_the_signing_host_on_the_channels_it_derives_from_the_new_keys() {
	let (host, responder) = sso_host_and_responder_sessions();
	let pending = PendingSessionRotation::start(&host, "rotate-1".to_string()).unwrap();
	let request = pending.request();
	let responder_identity = ResponderIdentity {
		statement_secret: responder.ss_secret,
		statement_public_key: responder.ss_public_key,
		encryption_secret_key: responder.enc_secret,
		encryption_public_key: X25519PrivateKey::from(responder.enc_secret).public_key().0,
	};
	let responder_rotated = establish_responder_session_info(
		&responder_identity,
		request.statement_account_id,
		request.encryption_public_key,
	)
	.unwrap();

	let rotated = pending.rotated(&host).unwrap();

	assert_eq!(
		(
			rotated.ss_public_key,
			rotated.session_id_own,
			rotated.session_id_peer,
			rotated.request_channel,
			rotated.peer_enc_pubkey,
			rotated.identity_account_id,
		),
		(
			request.statement_account_id,
			responder_rotated.session_id_peer,
			responder_rotated.session_id_own,
			responder_rotated.peer_request_channel,
			host.peer_enc_pubkey,
			host.identity_account_id,
		)
	);
	assert_ne!(
		rotated.session_id_own, host.session_id_own,
		"the change moves the session's channels"
	);
}

#[test]
fn a_stored_change_belongs_only_to_the_session_it_replaces() {
	let session = sso();
	let pending = PendingSessionRotation::start(&session, "rotate-1".to_string()).unwrap();
	let stored = pending.to_stored();
	let mut other = session.clone();
	other.ss_public_key = [0xEE; 32];
	let mut trailing = stored.clone();
	trailing.push(0);
	let mut newer_version = stored.clone();
	newer_version[0] = 1;

	assert_eq!(
		[
			PendingSessionRotation::from_stored(Some(&stored), &session),
			PendingSessionRotation::from_stored(Some(&stored), &other),
			PendingSessionRotation::from_stored(None, &session),
			PendingSessionRotation::from_stored(Some(&trailing), &session),
			PendingSessionRotation::from_stored(Some(&newer_version), &session),
		],
		[
			StoredPendingSessionRotation::Pending(Box::new(pending)),
			StoredPendingSessionRotation::Replaceable,
			StoredPendingSessionRotation::Replaceable,
			StoredPendingSessionRotation::Unreadable,
			StoredPendingSessionRotation::Unreadable,
		]
	);
}
