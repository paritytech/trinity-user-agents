use super::*;
use crate::session_usernames::SessionUsernames;
use futures::{FutureExt, StreamExt, executor::block_on};

fn info(pubkey_byte: u8) -> SessionInfo {
	SessionInfo {
		public_key: [pubkey_byte; 32],
		sso: None,
		root_entropy_source: None,
		identity_account_id: None,
		identity_chat_private_key: None,
		device_enc_public_key: None,
		lite_username: Some("alice".to_string()),
		full_username: None,
	}
}

/// The leading four fields every untagged layout shares.
///
/// Both helpers lay bytes down field by field rather than encoding a struct.
/// Encoding `SessionInfo` would make the fixtures follow it, so a field added
/// mid-struct would move the fixture and the assertion in step and the test
/// would keep passing while real blobs stopped decoding.
fn untagged_prefix(public_key: u8) -> Vec<u8> {
	let mut blob = Vec::new();
	blob.extend_from_slice(&[public_key; 32]);
	None::<SsoSessionInfo>.encode_to(&mut blob);
	None::<[u8; 32]>.encode_to(&mut blob);
	Some([0x22u8; 32]).encode_to(&mut blob);
	blob
}

/// An untagged blob carrying six fields and no identity material.
fn blob_before_identity_material(
	public_key: u8,
	lite_username: Option<&str>,
	full_username: Option<&str>,
) -> Vec<u8> {
	let mut blob = untagged_prefix(public_key);
	lite_username.map(str::to_owned).encode_to(&mut blob);
	full_username.map(str::to_owned).encode_to(&mut blob);
	blob
}

/// An untagged blob carrying all eight fields.
fn blob_with_identity_material(
	public_key: u8,
	chat_private_key: Option<[u8; 32]>,
	device_enc_public_key: Option<[u8; 32]>,
	lite_username: Option<&str>,
	full_username: Option<&str>,
) -> Vec<u8> {
	let mut blob = untagged_prefix(public_key);
	chat_private_key.encode_to(&mut blob);
	device_enc_public_key.encode_to(&mut blob);
	lite_username.map(str::to_owned).encode_to(&mut blob);
	full_username.map(str::to_owned).encode_to(&mut blob);
	blob
}

/// A six-field blob decodes, in every username shape. The pairing host
/// deletes a session it cannot decode, so failing here costs the pairing.
#[test]
fn a_six_field_session_decodes_in_every_username_shape() {
	for (label, lite, full) in [
		("both absent", None, None),
		("lite only", Some("alice.dot"), None),
		("both present", Some("alice.dot"), Some("Alice Smith")),
	] {
		let decoded = decode_persisted_session(&blob_before_identity_material(0x11, lite, full))
			.unwrap_or_else(|err| panic!("{label}: {err}"));
		assert_eq!(
			(
				decoded.public_key,
				decoded.lite_username.as_deref(),
				decoded.full_username.as_deref(),
				decoded.identity_account_id,
				decoded.identity_chat_private_key,
				decoded.device_enc_public_key,
			),
			([0x11; 32], lite, full, Some([0x22; 32]), None, None),
			"{label}: fields did not survive the older layout"
		);
	}
}

/// What a written blob encodes to is what the eight-field decoder reads.
///
/// The decoders are frozen descriptions of bytes on disk, so a field added
/// to [`SessionInfo`] must arrive as a new tagged version and must not be
/// added to them. Nothing else fails when the two drift: the decoders would
/// keep decoding and quietly drop whatever the new field carries.
#[test]
fn a_written_session_matches_the_eight_field_layout() {
	let mut live = info(0xa1);
	live.identity_chat_private_key = Some([0xa2; 32]);
	live.device_enc_public_key = Some([0xa3; 32]);

	const GUIDANCE: &str = "SessionInfo no longer matches the eight-field layout. Mint a \
         new PERSISTED_SESSION version and give it its own frozen layout; do not change \
         the existing ones, which describe bytes already on disk.";

	match decode_exact::<EightFieldSessionLayout>(&live.encode()) {
		Ok(Some(layout)) => assert_eq!(SessionInfo::from(layout), live, "{GUIDANCE}"),
		Ok(None) => panic!("{GUIDANCE} (it encodes to more bytes than the layout reads)"),
		Err(err) => panic!("{GUIDANCE} (the layout no longer decodes it: {err})"),
	}
}

/// A written blob carries every field of the live struct through the SSO
/// block, which the layouts embed by type rather than freezing field by
/// field. Pinning its encoded length catches a field added to it, which
/// would move every later field of both layouts.
#[test]
fn the_sso_block_is_the_length_both_layouts_expect() {
	const SSO_ENCODED_LEN: usize = 352;

	let sso = SsoSessionInfo {
		ss_secret: [0xb1; 64],
		ss_public_key: [0xb2; 32],
		enc_secret: [0xb3; 32],
		peer_enc_pubkey: [0xb4; 32],
		identity_account_id: [0xb5; 32],
		session_id_own: [0xb6; 32],
		session_id_peer: [0xb7; 32],
		request_channel: [0xb8; 32],
		response_channel: [0xb9; 32],
		peer_request_channel: [0xba; 32],
	};
	assert_eq!(
		sso.encode().len(),
		SSO_ENCODED_LEN,
		"the SSO block changed size, so both session layouts read different bytes; \
         mint a new PERSISTED_SESSION version rather than letting the layouts follow it"
	);

	let mut live = info(0xbb);
	live.sso = Some(sso);
	let restored = decode_persisted_session(&encode_persisted_session(&live))
		.expect("a session carrying SSO material round-trips");
	assert_eq!(restored, live);
}

/// An untagged blob carrying all eight fields keeps its identity material
/// rather than being read as the six-field layout and losing it.
#[test]
fn an_untagged_eight_field_session_keeps_its_identity_material() {
	let blob = blob_with_identity_material(
		0x77,
		Some([0x88; 32]),
		Some([0x99; 32]),
		Some("alice.dot"),
		None,
	);
	assert_ne!(
		blob.first(),
		Some(&PERSISTED_SESSION_V1),
		"the fixture must not accidentally look tagged"
	);

	let decoded = decode_persisted_session(&blob).expect("eight-field untagged blob decodes");

	assert_eq!(
		(
			decoded.public_key,
			decoded.identity_chat_private_key,
			decoded.device_enc_public_key,
			decoded.lite_username.as_deref(),
		),
		([0x77; 32], Some([0x88; 32]), Some([0x99; 32]), Some("alice.dot")),
		"identity material was dropped, so the blob was read as the six-field layout"
	);
}

/// The tag byte is not decisive on its own: `public_key` leads the untagged
/// layout and may legitimately begin with the same byte, so such a blob has
/// to reach the untagged arms rather than fail as a corrupt tagged one.
#[test]
fn an_untagged_session_whose_key_starts_with_the_tag_byte_still_decodes() {
	let blob = blob_before_identity_material(PERSISTED_SESSION_V1, Some("alice.dot"), None);
	assert_eq!(blob[0], PERSISTED_SESSION_V1);
	let decoded = decode_persisted_session(&blob).expect("decodes despite the leading byte");
	assert_eq!(decoded.public_key, [PERSISTED_SESSION_V1; 32]);
}

/// A blob this core writes carries the tag and round-trips unchanged.
#[test]
fn a_persisted_session_round_trips_through_the_tagged_layout() {
	let mut original = info(0x33);
	original.identity_chat_private_key = Some([0x44; 32]);
	original.device_enc_public_key = Some([0x55; 32]);

	let blob = encode_persisted_session(&original);

	assert_eq!(
		blob.first(),
		Some(&PERSISTED_SESSION_V1),
		"a written blob must carry the version tag"
	);
	assert_eq!(decode_persisted_session(&blob).expect("round trip"), original);
}

#[test]
fn a_blob_matching_no_known_layout_is_rejected() {
	assert!(decode_persisted_session(&[]).is_err());
	assert!(decode_persisted_session(&[PERSISTED_SESSION_V1, 0x00]).is_err());
	// A tagged blob with one byte too many is not silently truncated.
	let mut trailing = encode_persisted_session(&info(0x66));
	trailing.push(0x00);
	assert!(decode_persisted_session(&trailing).is_err());
}

#[test]
fn has_username_ignores_empty_usernames() {
	let mut session = info(0x42);
	session.lite_username = Some(String::new());
	session.full_username = None;
	assert!(!session.has_username());

	session.full_username = Some("Alice Smith".to_string());
	assert!(session.has_username());
}

#[test]
fn current_starts_empty() {
	let state = SessionState::new();
	assert!(state.current().is_none());
}

#[test]
fn set_then_current_returns_session() {
	let state = SessionState::new();
	state.set_session(info(0x42));
	let got = state.current().expect("session should be present");
	assert_eq!(got.public_key, [0x42; 32]);
	assert_eq!(got.lite_username.as_deref(), Some("alice"));
}

#[test]
fn replace_session_if_current_rejects_stale_expected_session() {
	let state = SessionState::new();
	let original = info(0x01);
	let replacement = info(0x02);
	state.set_session(original.clone());
	state.set_session(replacement.clone());

	assert!(!state.replace_session_if_current(&original, info(0x03)));
	assert_eq!(state.current(), Some(replacement));
}

#[test]
fn replace_session_if_current_updates_matching_session() {
	let state = SessionState::new();
	let original = info(0x01);
	let replacement = info(0x02);
	state.set_session(original.clone());

	assert!(state.replace_session_if_current(&original, replacement.clone()));

	assert_eq!(state.current(), Some(replacement));
}

#[test]
fn persisted_session_round_trips() {
	let mut session = info(0x42);
	session.root_entropy_source = Some([1; 32]);
	session.full_username = Some("Alice Smith".to_string());

	let blob = encode_persisted_session(&session);
	let decoded = decode_persisted_session(&blob).expect("session should decode");

	assert_eq!(decoded, session);
}

#[test]
fn debug_preserves_optional_secret_presence_without_exposing_values() {
	for entropy_present in [false, true] {
		for chat_key_present in [false, true] {
			let mut session = info(0x42);
			session.root_entropy_source = entropy_present.then_some([0xab; 32]);
			session.identity_chat_private_key = chat_key_present.then_some([0xcd; 32]);

			for rendered in [format!("{session:?}"), format!("{session:#?}")] {
				let compact: String = rendered.chars().filter(|ch| !ch.is_whitespace()).collect();
				for (field, present) in [
					("root_entropy_source", entropy_present),
					("identity_chat_private_key", chat_key_present),
				] {
					let expected = if present { "Some(\"<redacted>\"" } else { "None" };
					assert!(compact.contains(&format!("{field}:{expected}")));
				}
				assert!(!rendered.contains("171"), "entropy exposed");
				assert!(!rendered.contains("205"), "chat key exposed");
			}
		}
	}
}

#[test]
fn persisted_sso_session_round_trips() {
	let mut session = info(0x42);
	session.sso = Some(SsoSessionInfo {
		ss_secret: [1; 64],
		ss_public_key: [2; 32],
		enc_secret: [3; 32],
		peer_enc_pubkey: [4; 32],
		identity_account_id: [5; 32],
		session_id_own: [6; 32],
		session_id_peer: [7; 32],
		request_channel: [8; 32],
		response_channel: [9; 32],
		peer_request_channel: [10; 32],
	});

	session.root_entropy_source = Some([0xab; 32]);
	session.identity_chat_private_key = Some([0xcd; 32]);
	for rendered in [format!("{session:?}"), format!("{session:#?}")] {
		assert!(rendered.contains("<redacted>"));
		assert!(!rendered.contains("171"), "entropy exposed");
		assert!(!rendered.contains("205"), "chat key exposed");
		assert!(!rendered.contains("ss_secret: ["), "statement key exposed");
		assert!(!rendered.contains("enc_secret: ["), "encryption key exposed");
	}
	let blob = encode_persisted_session(&session);
	let decoded = decode_persisted_session(&blob).expect("session should decode");

	assert_eq!(decoded, session);
}

#[test]
fn external_paired_session_uses_canonical_shape_and_exact_fields() {
	let external = ExternalPairedSession {
		root_public_key: [11; 32],
		sso: SsoSessionInfo {
			ss_secret: [1; 64],
			ss_public_key: [2; 32],
			enc_secret: [3; 32],
			peer_enc_pubkey: [4; 32],
			identity_account_id: [5; 32],
			session_id_own: [6; 32],
			session_id_peer: [7; 32],
			request_channel: [8; 32],
			response_channel: [9; 32],
			peer_request_channel: [10; 32],
		},
		root_entropy_source: [12; 32],
		identity_account_id: [5; 32],
		identity_chat_private_key: Some([13; 32]),
		device_enc_public_key: Some([14; 32]),
	};

	let blob = encode_external_paired_session(external);
	let decoded = decode_persisted_session(&blob).expect("canonical decoder accepts blob");

	assert_eq!(
		decoded,
		SessionInfo {
			public_key: [11; 32],
			sso: Some(SsoSessionInfo {
				ss_secret: [1; 64],
				ss_public_key: [2; 32],
				enc_secret: [3; 32],
				peer_enc_pubkey: [4; 32],
				identity_account_id: [5; 32],
				session_id_own: [6; 32],
				session_id_peer: [7; 32],
				request_channel: [8; 32],
				response_channel: [9; 32],
				peer_request_channel: [10; 32],
			}),
			root_entropy_source: Some([12; 32]),
			identity_account_id: Some([5; 32]),
			identity_chat_private_key: Some([13; 32]),
			device_enc_public_key: Some([14; 32]),
			lite_username: None,
			full_username: None,
		}
	);

	let mut wrong_shape = blob;
	wrong_shape.push(0);
	assert!(decode_persisted_session(&wrong_shape).is_err());
}

#[test]
fn persisted_session_rejects_trailing_bytes() {
	let mut blob = encode_persisted_session(&info(0x42));
	blob.push(0);

	let err = decode_persisted_session(&blob).unwrap_err();

	assert_eq!(err, "invalid session blob: trailing bytes");
}

#[test]
fn clear_returns_to_empty() {
	let state = SessionState::new();
	state.set_session(info(0x01));
	state.clear_session();
	assert!(state.current().is_none());
}

#[test]
fn subscribe_emits_current_state_first() {
	let state = SessionState::new();
	state.set_session(info(0x01));
	let mut stream = state.subscribe();
	let first = block_on(stream.next()).expect("expected initial item");
	assert_eq!(first, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected));
}

#[test]
fn subscribe_emits_disconnected_when_no_session() {
	let state = SessionState::new();
	let mut stream = state.subscribe();
	let first = block_on(stream.next()).expect("expected initial item");
	assert_eq!(first, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Disconnected));
}

#[test]
fn set_session_broadcasts_connected_to_existing_subscribers() {
	let state = SessionState::new();
	let mut stream = state.subscribe();
	let _ = block_on(stream.next());

	state.set_session(info(0x01));
	let next = block_on(stream.next()).expect("expected Connected event");
	assert_eq!(next, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected));
}

#[test]
fn clear_session_broadcasts_disconnected_to_existing_subscribers() {
	let state = SessionState::new();
	state.set_session(info(0x01));
	let mut stream = state.subscribe();
	let _ = block_on(stream.next());

	state.clear_session();
	let next = block_on(stream.next()).expect("expected Disconnected event");
	assert_eq!(next, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Disconnected));
}

#[test]
fn set_session_with_same_info_does_not_re_emit_connected() {
	let state = SessionState::new();
	state.set_session(info(0x01));
	let mut stream = state.subscribe();
	let _ = block_on(stream.next());

	state.set_session(info(0x01));

	let pending = stream.next().now_or_never();
	assert!(pending.is_none(), "no transition event expected for equivalent session");
}

#[test]
fn set_session_with_replacement_re_emits_connected() {
	let state = SessionState::new();
	state.set_session(info(0x01));
	let mut stream = state.subscribe();
	let _ = block_on(stream.next());

	state.set_session(info(0x02));

	let next = block_on(stream.next()).expect("expected replacement Connected event");
	assert_eq!(next, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected));
}

#[test]
fn multi_subscriber_broadcast() {
	let state = SessionState::new();
	let mut a = state.subscribe();
	let mut b = state.subscribe();
	// Drain initial Disconnected from both.
	let _ = block_on(a.next());
	let _ = block_on(b.next());

	state.set_session(info(0x77));
	let a_next = block_on(a.next()).expect("a should receive Connected");
	let b_next = block_on(b.next()).expect("b should receive Connected");
	assert_eq!(a_next, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected));
	assert_eq!(b_next, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected));
}

/// Clearing a never-set session is a no-op and must not synthesize a
/// spurious `Disconnected` event for live subscribers.
#[test]
fn clear_when_empty_is_silent_no_op() {
	let state = SessionState::new();
	let mut stream = state.subscribe();
	// Drain the initial Disconnected.
	let _ = block_on(stream.next());

	state.clear_session();

	let pending = stream.next().now_or_never();
	assert!(pending.is_none(), "no event expected when clear is a no-op",);
}

/// Dropping a subscriber's stream must remove that sender from the
/// broadcast list. The next broadcast prunes it; the surviving stream
/// still receives the event.
#[test]
fn dropped_subscriber_is_pruned() {
	let state = SessionState::new();
	let mut survivor = state.subscribe();
	let dropping = state.subscribe();
	let _ = block_on(survivor.next());
	// Drain the initial item from the dropping stream too so we don't
	// accidentally test buffered-but-undelivered.
	drop(dropping);

	state.set_session(info(0x33));
	let next = block_on(survivor.next()).expect("survivor must receive Connected");
	assert_eq!(next, VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected),);

	// Internally, `set_session`'s broadcast call `retain`-prunes any
	// dropped senders. After the call the subscribers list should have
	// exactly one entry (the survivor).
	let inner = state.inner.lock().unwrap();
	assert_eq!(inner.subscribers.len(), 1, "dropped subscriber not pruned");
}
