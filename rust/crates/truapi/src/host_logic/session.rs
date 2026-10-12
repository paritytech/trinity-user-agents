//! Pairing-host active-session state. The runtime updates this when pairing or
//! unpairing with a signing host changes the inter-host session, and
//! account-management methods read it instead of round-tripping host callbacks
//! on every product call.
//!
//! Host-spec B.1.5 and B.3.1 define the remote account keys and session topics:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L85-L103>
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L119-L131>
//! The persisted blob is core-owned and host-local; storage.md captures current
//! cross-host persistence status quo:
//! <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/storage.md?plain=1#L58-L98>

use futures::channel::mpsc;
use futures::stream::{self, BoxStream, StreamExt};
use parity_scale_codec::{Decode, Encode};
use std::sync::{Arc, Mutex};

use truapi::v01::HostAccountConnectionStatusSubscribeItem;
use truapi::versioned::account::HostAccountConnectionStatusSubscribeItem as VersionedItem;

/// Session info for a pairing host's active signing-host session. The 32-byte
/// sr25519 public key plus optional usernames are sourced from the signing host
/// and dotNS identity record (read from Asset Hub).
#[derive(derive_more::Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct SessionInfo {
    /// 32-byte sr25519 root public key owned by the signing host.
    pub public_key: [u8; 32],
    /// SSO channel state negotiated by this pairing host with the signing host.
    /// Sessions restored from older test fixtures may leave it empty.
    pub sso: Option<SsoSessionInfo>,
    /// Wallet-provided source for deterministic product entropy.
    #[debug("{:?}", root_entropy_source.as_ref().map(|_| "<redacted>"))]
    pub root_entropy_source: Option<[u8; 32]>,
    /// Wallet identity account id used for the dotNS username lookup on Asset Hub.
    pub identity_account_id: Option<[u8; 32]>,
    /// X25519 private key addressing this identity in chat. A pairing host
    /// retains what the handshake shares and cannot recompute it.
    #[debug("{:?}", identity_chat_private_key.as_ref().map(|_| "<redacted>"))]
    pub identity_chat_private_key: Option<[u8; 32]>,
    /// X25519 public key of the wallet device that answered pairing. Distinct
    /// from [`SsoSessionInfo::peer_enc_pubkey`], which keys the SSO channels.
    pub device_enc_public_key: Option<[u8; 32]>,
    /// Short username (e.g. `alice`).
    pub lite_username: Option<String>,
    /// Fully qualified username (e.g. `Alice Smith`).
    pub full_username: Option<String>,
}

impl crate::session_usernames::SessionUsernames for SessionInfo {
    fn has_username(&self) -> bool {
        non_empty_username(&self.full_username) || non_empty_username(&self.lite_username)
    }

    fn apply_usernames(&mut self, lite_username: Option<String>, full_username: Option<String>) {
        if non_empty_username(&full_username) {
            self.full_username = full_username;
        }
        if non_empty_username(&lite_username) {
            self.lite_username = lite_username;
        }
    }
}

fn non_empty_username(value: &Option<String>) -> bool {
    value.as_ref().is_some_and(|value| !value.is_empty())
}

/// SSO session material negotiated by the pairing host with the signing host.
#[derive(
    derive_more::Debug,
    Clone,
    PartialEq,
    Eq,
    Encode,
    Decode,
    zeroize::Zeroize,
    zeroize::ZeroizeOnDrop,
)]
pub struct SsoSessionInfo {
    /// Pairing host's own 64-byte expanded sr25519 statement-store secret.
    #[debug("\"<redacted>\"")]
    pub ss_secret: [u8; 64],
    /// Pairing host's own session sr25519 statement-store public key.
    pub ss_public_key: [u8; 32],
    /// Pairing host's X25519 private key.
    #[debug("\"<redacted>\"")]
    pub enc_secret: [u8; 32],
    /// Signing host's persistent X25519 public key.
    pub peer_enc_pubkey: [u8; 32],
    /// Signing host's identity sr25519 account id.
    pub identity_account_id: [u8; 32],
    /// Pairing host -> signing host topic id.
    pub session_id_own: [u8; 32],
    /// Signing host -> pairing host topic id.
    pub session_id_peer: [u8; 32],
    /// Statement channel for pairing-host requests.
    pub request_channel: [u8; 32],
    /// Statement channel for signing-host responses to pairing-host requests.
    pub response_channel: [u8; 32],
    /// Statement channel for signing-host initiated requests.
    pub peer_request_channel: [u8; 32],
}
/// Session fields supplied by an already-paired external host runtime.
///
/// This is an input shape, not a second persistence format. Encoding always
/// goes through [`encode_persisted_session`] so callers cannot duplicate or
/// depend on the private SCALE layout of [`SessionInfo`].
pub struct ExternalPairedSession {
    /// Signing host's sr25519 root public key.
    pub root_public_key: [u8; 32],
    /// Pairing host's established SSO channel and key material.
    pub sso: SsoSessionInfo,
    /// Wallet-provided source for deterministic product entropy.
    pub root_entropy_source: [u8; 32],
    /// Wallet identity account id used for the dotNS username lookup on Asset Hub.
    pub identity_account_id: [u8; 32],
    /// Wallet-supplied X25519 private key addressing this identity in chat,
    /// when the external runtime captured it during its own handshake.
    pub identity_chat_private_key: Option<[u8; 32]>,
    /// X25519 public key of the wallet device that answered pairing, when the
    /// external runtime captured it during its own handshake.
    pub device_enc_public_key: Option<[u8; 32]>,
}

/// Encode an already-paired external host session as the canonical opaque
/// pairing-runtime session blob.
///
/// Usernames are intentionally absent: the pairing runtime resolves and
/// persists them through its normal identity lookup path.
pub fn encode_external_paired_session(info: ExternalPairedSession) -> Vec<u8> {
    encode_persisted_session(&SessionInfo {
        public_key: info.root_public_key,
        sso: Some(info.sso),
        root_entropy_source: Some(info.root_entropy_source),
        identity_account_id: Some(info.identity_account_id),
        identity_chat_private_key: info.identity_chat_private_key,
        device_enc_public_key: info.device_enc_public_key,
        lite_username: None,
        full_username: None,
    })
}

/// Leading byte on every session blob this core writes.
///
/// The blob is a bare SCALE struct, so its layout is positional: inserting a
/// field changes where every later field starts and silently invalidates
/// everything already on disk. The tag makes the layout explicit, so a future
/// field can be added by minting a new version rather than by breaking readers.
const PERSISTED_SESSION_V1: u8 = 1;

/// An untagged blob's eight fields, in the order an untagged blob carries them.
///
/// Read only, and frozen: it is a snapshot of a byte layout that exists on disk,
/// not a view of [`SessionInfo`]. Probing the live struct instead would follow
/// every future field change and stop decoding the blobs this exists to read.
#[derive(Decode)]
struct EightFieldSessionLayout {
    public_key: [u8; 32],
    sso: Option<SsoSessionInfo>,
    root_entropy_source: Option<[u8; 32]>,
    identity_account_id: Option<[u8; 32]>,
    identity_chat_private_key: Option<[u8; 32]>,
    device_enc_public_key: Option<[u8; 32]>,
    lite_username: Option<String>,
    full_username: Option<String>,
}

/// An untagged blob's six fields, carrying no identity material.
///
/// Read only, and frozen for the same reason as
/// [`EightFieldSessionLayout`].
#[derive(Decode)]
struct SixFieldSessionLayout {
    public_key: [u8; 32],
    sso: Option<SsoSessionInfo>,
    root_entropy_source: Option<[u8; 32]>,
    identity_account_id: Option<[u8; 32]>,
    lite_username: Option<String>,
    full_username: Option<String>,
}

impl From<EightFieldSessionLayout> for SessionInfo {
    fn from(blob: EightFieldSessionLayout) -> Self {
        Self {
            public_key: blob.public_key,
            sso: blob.sso,
            root_entropy_source: blob.root_entropy_source,
            identity_account_id: blob.identity_account_id,
            identity_chat_private_key: blob.identity_chat_private_key,
            device_enc_public_key: blob.device_enc_public_key,
            lite_username: blob.lite_username,
            full_username: blob.full_username,
        }
    }
}

impl From<SixFieldSessionLayout> for SessionInfo {
    fn from(blob: SixFieldSessionLayout) -> Self {
        Self {
            public_key: blob.public_key,
            sso: blob.sso,
            root_entropy_source: blob.root_entropy_source,
            identity_account_id: blob.identity_account_id,
            // This layout carries no identity material, and a pairing host cannot
            // recompute either value, so chat and device-addressed features are
            // unavailable for the session until it pairs again.
            identity_chat_private_key: None,
            device_enc_public_key: None,
            lite_username: blob.lite_username,
            full_username: blob.full_username,
        }
    }
}

/// Decode `T` from `blob`. `Ok(None)` reports that `T` parsed a prefix and left
/// bytes over, which is what distinguishes a corrupt blob from a shorter layout.
fn decode_exact<T: Decode>(blob: &[u8]) -> Result<Option<T>, String> {
    let mut input = blob;
    let decoded = T::decode(&mut input).map_err(|err| err.to_string())?;
    Ok(input.is_empty().then_some(decoded))
}

/// Encode the active-session fields the core currently understands into an
/// opaque host-global session blob.
pub fn encode_persisted_session(info: &SessionInfo) -> Vec<u8> {
    let mut blob = Vec::new();
    blob.push(PERSISTED_SESSION_V1);
    info.encode_to(&mut blob);
    blob
}

/// Decode a core-owned persisted session blob.
///
/// Takes the first layout that consumes the blob exactly: the tagged one, then
/// each untagged one. The tag is checked first but is not decisive, because
/// `public_key` leads an untagged blob and may legitimately begin with the tag
/// byte, so a failed tagged read still reaches the untagged layouts.
///
/// A failure here deletes the stored blob (see the caller in `pairing_host`), so
/// every layout is tried before any is reported, and no single arm can end the
/// search early.
pub fn decode_persisted_session(blob: &[u8]) -> Result<SessionInfo, String> {
    let mut failures = Vec::new();
    // A tagged blob names its own layout, so bytes left over after it are
    // corruption of a known shape rather than a hint to try another. That
    // verdict is reported on its own, and only once every layout has been tried.
    let mut tagged_trailing = false;

    if let Some((&PERSISTED_SESSION_V1, body)) = blob.split_first() {
        match decode_exact::<EightFieldSessionLayout>(body) {
            Ok(Some(layout)) => return Ok(layout.into()),
            Ok(None) => tagged_trailing = true,
            Err(err) => failures.push(format!("tagged v{PERSISTED_SESSION_V1}: {err}")),
        }
    }
    match decode_exact::<EightFieldSessionLayout>(blob) {
        Ok(Some(layout)) => return Ok(layout.into()),
        Ok(None) => failures.push("untagged, eight fields: trailing bytes".to_string()),
        Err(err) => failures.push(format!("untagged, eight fields: {err}")),
    }
    match decode_exact::<SixFieldSessionLayout>(blob) {
        Ok(Some(layout)) => return Ok(layout.into()),
        Ok(None) => failures.push("untagged, six fields: trailing bytes".to_string()),
        Err(err) => failures.push(format!("untagged, six fields: {err}")),
    }

    if tagged_trailing {
        return Err("invalid session blob: trailing bytes".to_string());
    }
    Err(format!(
        "invalid session blob: no known layout decodes it ({})",
        failures.join("; ")
    ))
}

/// Holds the currently-active session and broadcasts connection-status
/// transitions to subscribers. Cheap to clone via `Arc`.
#[derive(Default)]
pub struct SessionState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    current: Option<SessionInfo>,
    subscribers: Vec<mpsc::UnboundedSender<VersionedItem>>,
}

impl SessionState {
    /// Construct a fresh session holder, starting in the `Disconnected` state.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Replace the active session with `info`. Emits a `Connected` event to
    /// every live subscriber if this is a transition from no-session or an
    /// actual session replacement.
    pub fn set_session(&self, info: SessionInfo) {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        let should_broadcast = inner.current.as_ref() != Some(&info);
        inner.current = Some(info);
        if should_broadcast {
            broadcast(
                &mut inner.subscribers,
                HostAccountConnectionStatusSubscribeItem::Connected,
            );
        }
    }

    /// Replace the active session only when it still matches `expected`.
    pub fn replace_session_if_current(&self, expected: &SessionInfo, info: SessionInfo) -> bool {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        if inner.current.as_ref() != Some(expected) {
            return false;
        }

        let should_broadcast = inner.current.as_ref() != Some(&info);
        inner.current = Some(info);
        if should_broadcast {
            broadcast(
                &mut inner.subscribers,
                HostAccountConnectionStatusSubscribeItem::Connected,
            );
        }
        true
    }

    /// Drop the active session. Emits a `Disconnected` event to every live
    /// subscriber if there was a session to clear.
    pub fn clear_session(&self) {
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        if inner.current.take().is_some() {
            broadcast(
                &mut inner.subscribers,
                HostAccountConnectionStatusSubscribeItem::Disconnected,
            );
        }
    }

    /// Snapshot of the current session, or `None` when nothing is paired.
    pub fn current(&self) -> Option<SessionInfo> {
        self.inner
            .lock()
            .expect("session-state mutex poisoned")
            .current
            .clone()
    }

    /// Stream of connection-status events. The first item emitted is the
    /// current state (so subscribers don't have to read it separately);
    /// subsequent items reflect every `set_session` / `clear_session`
    /// transition.
    pub fn subscribe(&self) -> BoxStream<'static, VersionedItem> {
        let (tx, rx) = mpsc::unbounded();
        let mut inner = self.inner.lock().expect("session-state mutex poisoned");
        let initial = match inner.current {
            Some(_) => HostAccountConnectionStatusSubscribeItem::Connected,
            None => HostAccountConnectionStatusSubscribeItem::Disconnected,
        };
        inner.subscribers.push(tx);
        let initial_item = VersionedItem::V1(initial);
        Box::pin(stream::once(async move { initial_item }).chain(rx))
    }
}

/// Broadcast one connection-status transition and prune dropped subscribers.
fn broadcast(
    subscribers: &mut Vec<mpsc::UnboundedSender<VersionedItem>>,
    status: HostAccountConnectionStatusSubscribeItem,
) {
    let item = VersionedItem::V1(status);
    // `retain` drops senders whose receiver has been dropped, so the
    // subscriber list self-prunes on the next broadcast after a reader
    // unsubscribes.
    subscribers.retain(|tx| tx.unbounded_send(item.clone()).is_ok());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_usernames::SessionUsernames;
    use futures::executor::block_on;
    use futures::{FutureExt, StreamExt};

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
            let decoded =
                decode_persisted_session(&blob_before_identity_material(0x11, lite, full))
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
            (
                [0x77; 32],
                Some([0x88; 32]),
                Some([0x99; 32]),
                Some("alice.dot")
            ),
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
        assert_eq!(
            decode_persisted_session(&blob).expect("round trip"),
            original
        );
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
    fn session_username_helpers_check_and_apply_non_empty_values() {
        let mut session = info(0x42);
        session.lite_username = None;
        session.full_username = None;

        assert!(!session.has_username());

        session.apply_usernames(Some(String::new()), Some("Alice Smith".to_string()));
        assert!(session.has_username());
        assert_eq!(session.full_username.as_deref(), Some("Alice Smith"));
        assert_eq!(session.lite_username, None);

        session.apply_usernames(Some("alice".to_string()), Some(String::new()));
        assert_eq!(session.full_username.as_deref(), Some("Alice Smith"));
        assert_eq!(session.lite_username.as_deref(), Some("alice"));
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
                    let compact: String =
                        rendered.chars().filter(|ch| !ch.is_whitespace()).collect();
                    for (field, present) in [
                        ("root_entropy_source", entropy_present),
                        ("identity_chat_private_key", chat_key_present),
                    ] {
                        let expected = if present {
                            "Some(\"<redacted>\""
                        } else {
                            "None"
                        };
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
            assert!(
                !rendered.contains("enc_secret: ["),
                "encryption key exposed"
            );
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
        assert_eq!(
            first,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
    }

    #[test]
    fn subscribe_emits_disconnected_when_no_session() {
        let state = SessionState::new();
        let mut stream = state.subscribe();
        let first = block_on(stream.next()).expect("expected initial item");
        assert_eq!(
            first,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Disconnected)
        );
    }

    #[test]
    fn set_session_broadcasts_connected_to_existing_subscribers() {
        let state = SessionState::new();
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.set_session(info(0x01));
        let next = block_on(stream.next()).expect("expected Connected event");
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
    }

    #[test]
    fn clear_session_broadcasts_disconnected_to_existing_subscribers() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.clear_session();
        let next = block_on(stream.next()).expect("expected Disconnected event");
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Disconnected)
        );
    }

    #[test]
    fn set_session_with_same_info_does_not_re_emit_connected() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.set_session(info(0x01));

        let pending = stream.next().now_or_never();
        assert!(
            pending.is_none(),
            "no transition event expected for equivalent session"
        );
    }

    #[test]
    fn set_session_with_replacement_re_emits_connected() {
        let state = SessionState::new();
        state.set_session(info(0x01));
        let mut stream = state.subscribe();
        let _ = block_on(stream.next());

        state.set_session(info(0x02));

        let next = block_on(stream.next()).expect("expected replacement Connected event");
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
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
        assert_eq!(
            a_next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
        assert_eq!(
            b_next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected)
        );
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
        assert_eq!(
            next,
            VersionedItem::V1(HostAccountConnectionStatusSubscribeItem::Connected),
        );

        // Internally, `set_session`'s broadcast call `retain`-prunes any
        // dropped senders. After the call the subscribers list should have
        // exactly one entry (the survivor).
        let inner = state.inner.lock().unwrap();
        assert_eq!(inner.subscribers.len(), 1, "dropped subscriber not pruned");
    }
}
