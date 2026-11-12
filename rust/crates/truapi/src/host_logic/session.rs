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

use futures::{
	channel::mpsc,
	stream::{self, BoxStream, StreamExt},
};
use parity_scale_codec::{Decode, Encode};
use std::sync::{Arc, Mutex};

use truapi::{
	v01::HostAccountConnectionStatusSubscribeItem,
	versioned::account::HostAccountConnectionStatusSubscribeItem as VersionedItem,
};

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
fn decode_exact<T: Decode>(blob: &[u8]) -> Result<Option<T>, parity_scale_codec::Error> {
	let mut input = blob;
	let decoded = T::decode(&mut input)?;
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
	Err(format!("invalid session blob: no known layout decodes it ({})", failures.join("; ")))
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
			broadcast(&mut inner.subscribers, HostAccountConnectionStatusSubscribeItem::Connected);
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
			broadcast(&mut inner.subscribers, HostAccountConnectionStatusSubscribeItem::Connected);
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
		self.inner.lock().expect("session-state mutex poisoned").current.clone()
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
mod tests;
