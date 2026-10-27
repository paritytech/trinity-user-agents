//! Core-resolved public profiles (RFC "Core-resolved public profiles").
//!
//! A host asks for one person's profile by the label its contact row holds. The core reads the
//! `profile` text record under that name with the walk it makes for product manifests, decodes
//! it from the same definition the writer encodes with, fetches the avatar bytes through the
//! host's preimage backend and keeps them only if they hash to the CID, and answers with the
//! profile, a definite absence, or a failure. The decoded record is cached for an hour; the
//! bytes are the host's to keep, because Bulletin drops blobs after 14 days.

use core::time::Duration;

use futures::StreamExt;
use parity_scale_codec::{Decode, Encode};
use tracing::{info, instrument, warn};
use truapi::v01::{PROFILE_TEXT_RECORD_KEY, ProfileAvatar, ProfileAvatarFormat, ProfileRecord};

use crate::{
	dotns_views::tld_node,
	host_internal::bulletin::{preimage_key, preimage_key_from_cid},
	host_logic::dotns_gateway::{DotnsError, DotnsTransport, namehash_under},
	platform::{CoreStorageKey, Platform, PreimageHost},
	runtime::{
		dotns_lookup::DotnsLookup,
		product_manifest::{text_record, with_ceiling},
		services::RuntimeServices,
	},
	unix_time::current_unix_secs,
};

/// How long a resolved record is honoured before the chain is read again. A change or a
/// takedown reaches a host within this, and a forced lookup skips it.
pub const PUBLIC_PROFILE_TTL_SECS: u64 = 60 * 60;

/// The chain walk, protocol registry to text record, gives up after this.
const LOOKUP_CEILING: Duration = Duration::from_secs(30);
/// The avatar bytes are waited for this long; the host's preimage lookup answers a miss first
/// and may find the bytes later.
const AVATAR_CEILING: Duration = Duration::from_secs(60);

/// What the core answers a host that asked for a person's profile.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PublicProfileLookup {
	/// The name has a record. Either field may be absent.
	Found {
		/// The avatar, fetched and checked; absent when the record names none or the bytes
		/// could not be had this time.
		avatar: Option<ResolvedAvatar>,
		/// The display name.
		name: Option<String>,
	},
	/// The name has no record, or none the core can read. A host clears what it stored.
	Absent,
	/// The core could not tell: the chain was out of reach. A host keeps what it has and
	/// asks again next time.
	Failed {
		/// What went wrong, for a log.
		reason: String,
	},
}

/// An avatar the core fetched and checked.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ResolvedAvatar {
	/// For change detection; a host stores it beside the bytes.
	pub cid: String,
	/// The image format the record declares.
	pub format: ProfileAvatarFormat,
	/// The image bytes, which hash to `cid`.
	pub bytes: Vec<u8>,
}

/// What the core keeps of a record between lookups. `record` is `None` where the name had
/// no record: a miss is cached too, since asking again within the lifetime answers the same.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct CachedProfile {
	fetched_at_secs: u64,
	record: Option<CachedRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
struct CachedRecord {
	avatar: Option<ProfileAvatar>,
	name: Option<String>,
}

impl CachedRecord {
	fn of(record: &ProfileRecord) -> Self {
		Self { avatar: record.avatar(), name: record.name() }
	}
}

/// Standard base64 with padding, the one form the writer emits. `None` for anything else, so
/// a record that is not base64 reads as no record. Written here because the `base64` crate is
/// not linked on wasm32, where this runtime also runs.
pub(crate) fn decode_base64(text: &str) -> Option<Vec<u8>> {
	const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
	let unpadded = text.trim_end_matches('=');
	if (text.len() - unpadded.len()) > 2 || !text.len().is_multiple_of(4) {
		return None;
	}
	let mut bytes = Vec::with_capacity(unpadded.len() * 3 / 4);
	let (mut buffer, mut bits) = (0u32, 0u32);
	for digit in unpadded.bytes() {
		let value = ALPHABET.iter().position(|&c| c == digit)? as u32;
		buffer = (buffer << 6) | value;
		bits += 6;
		if bits >= 8 {
			bytes.push(((buffer >> (bits - 8)) & 0xff) as u8);
			bits -= 8;
		}
	}
	// The bits a final partial group leaves over must be zero padding.
	(buffer & ((1 << bits) - 1) == 0).then_some(bytes)
}

/// The node of `<label>.<tld>` as ENS hashes a dotted name: every part is one level, so the
/// lite name `alice.42` is `alice` under the `42` container, which is how dotNS registers it
/// and how the writer derived the node it wrote under.
pub fn name_node(tld: &str, label: &str) -> [u8; 32] {
	label
		.rsplit('.')
		.fold(tld_node(tld), |parent, part| namehash_under(&parent, part))
}

/// `label` without a trailing `.<tld>`. `tld` is as `network_tld` reports it, with its
/// leading dot.
fn bare_label<'a>(label: &'a str, tld: &str) -> &'a str {
	let suffix = format!(".{}", tld.trim_start_matches('.'));
	label.strip_suffix(suffix.as_str()).unwrap_or(label)
}

fn acceptable_label(label: &str) -> bool {
	!label.is_empty() &&
		label.len() <= 64 &&
		label.split('.').all(|part| {
			!part.is_empty() &&
				part.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
		})
}

/// Read and decode the `profile` record under `label`. `Ok(None)` where there is no record or
/// what is there is not a profile; a chain failure is the error.
pub(crate) async fn read_record<T: DotnsTransport + ?Sized>(
	transport: &mut T,
	label: &str,
) -> Result<Option<ProfileRecord>, DotnsError> {
	let Some(text) = text_record(transport, PROFILE_TEXT_RECORD_KEY, |tld| {
		name_node(tld, bare_label(label, tld))
	})
	.await?
	else {
		return Ok(None);
	};
	let Some(bytes) = decode_base64(text.trim()) else {
		warn!(label, "the profile record is not base64; treating it as none");
		return Ok(None);
	};
	match ProfileRecord::decode_bounded(&bytes) {
		Ok(record) => Ok(Some(record)),
		Err(error) => {
			warn!(label, ?error, "the profile record does not decode; treating it as none");
			Ok(None)
		},
	}
}

/// The avatar bytes for `cid` through the host's preimage backend, kept only if they hash to
/// it. `None` is "not this time": the record still names the avatar, and the next lookup asks
/// again.
pub(crate) async fn fetch_avatar_bytes(platform: &dyn PreimageHost, cid: &str) -> Option<Vec<u8>> {
	let Some(key) = preimage_key_from_cid(cid) else {
		warn!(cid, "the avatar CID is not a Bulletin preimage CID");
		return None;
	};
	let found = platform.lookup_preimage(key).filter_map(|item| async move {
		match item {
			Ok(Some(bytes)) if preimage_key(&bytes) == key => Some(bytes),
			Ok(Some(_)) => {
				warn!(cid, "the preimage backend answered bytes that do not hash to the CID");
				None
			},
			Ok(None) => None,
			Err(error) => {
				warn!(cid, reason = %error.reason, "the preimage backend failed");
				None
			},
		}
	});
	futures::pin_mut!(found);
	with_ceiling(AVATAR_CEILING, found.next()).await.flatten()
}

async fn cached(platform: &dyn Platform, key: &CoreStorageKey, now: u64) -> Option<CachedProfile> {
	let bytes = platform.read_core_storage(key.clone()).await.ok()??;
	let cached = CachedProfile::decode(&mut bytes.as_slice()).ok()?;
	// A stamp ahead of the clock is stale, not fresh forever: see `root_manifest`.
	(cached.fetched_at_secs <= now && now - cached.fetched_at_secs < PUBLIC_PROFILE_TTL_SECS)
		.then_some(cached)
}

/// Resolve the public profile under `label`, a person's dotNS name without its TLD (`alice`
/// or `alice.42`; a trailing `.<tld>` is tolerated). `force` reads the chain regardless of
/// the cache.
#[instrument(skip_all, fields(runtime.method = "public_profile.resolve", label))]
pub async fn resolve_public_profile(
	services: &RuntimeServices,
	label: &str,
	force: bool,
) -> PublicProfileLookup {
	let label = label.trim().to_ascii_lowercase();
	if !acceptable_label(&label) {
		return PublicProfileLookup::Failed { reason: format!("{label:?} is not a dotNS name") };
	}
	let platform = services.platform.as_ref();
	let key = CoreStorageKey::public_profile(&label);
	let now = current_unix_secs();

	let record = match if force { None } else { cached(platform, &key, now).await } {
		Some(cached) => cached.record,
		None => {
			let Some(genesis_hash) = services.asset_hub_chain_genesis_hash() else {
				return PublicProfileLookup::Failed {
					reason: "the host serves no Asset Hub".into(),
				};
			};
			let read = with_ceiling(LOOKUP_CEILING, async {
				let mut lookup = DotnsLookup::pinned_to_best_block(
					&services.chain,
					genesis_hash,
					&format!("profile:{label}"),
				)
				.await?;
				read_record(&mut lookup, &label).await
			})
			.await;
			let record = match read {
				None => {
					return PublicProfileLookup::Failed {
						reason: "the profile lookup timed out".into(),
					};
				},
				Some(Err(error)) => {
					warn!(reason = %error.reason(), "profile lookup failed");
					return PublicProfileLookup::Failed { reason: error.reason() };
				},
				Some(Ok(record)) => record.as_ref().map(CachedRecord::of),
			};
			let _ = platform
				.write_core_storage(
					key,
					CachedProfile { fetched_at_secs: now, record: record.clone() }.encode(),
				)
				.await;
			record
		},
	};

	let Some(record) = record else {
		return PublicProfileLookup::Absent;
	};
	let avatar = match record.avatar {
		Some(avatar) => {
			let bytes = match preimage_key_from_cid(&avatar.cid)
				.and_then(|key| services.cached_preimage(&key))
			{
				Some(bytes) => Some(bytes),
				None => {
					let bytes = fetch_avatar_bytes(platform, &avatar.cid).await;
					if let (Some(bytes), Some(key)) = (&bytes, preimage_key_from_cid(&avatar.cid)) {
						services.cache_preimage(key, bytes.clone());
					}
					bytes
				},
			};
			bytes.map(|bytes| ResolvedAvatar { cid: avatar.cid, format: avatar.format, bytes })
		},
		None => None,
	};
	info!(found_avatar = avatar.is_some(), has_name = record.name.is_some(), "profile resolved");
	PublicProfileLookup::Found { avatar, name: record.name }
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;

	use super::*;
	use crate::{
		host_internal::bulletin::preimage_cid,
		host_logic::dotns_gateway::{DotnsCallError, DotnsContractError, DotnsViewError, selector},
		platform::CoreStorage,
		test_support::StubPlatform,
	};

	// What seity's profile-core writes for { avatar: (CID of 0xab × 32, webp), name: "Alice" }.
	const VECTOR_BASE64: &str = "CBhhdmF0YXIBAfhiYWZrMmJ6YWNlY3YyeGs1bHZvdjJ4azVsdm92MnhrNWx2b3YyeGs1bHZvdjJ4azVsdm92MnhrNWx2b3YydwIQbmFtZRRBbGljZQ==";
	const VECTOR_CID: &str = "bafk2bzacecv2xk5lvov2xk5lvov2xk5lvov2xk5lvov2xk5lvov2xk5lvov2w";

	fn abi_word(value: u64) -> [u8; 32] {
		let mut word = [0u8; 32];
		word[24..].copy_from_slice(&value.to_be_bytes());
		word
	}

	fn abi_string(value: &str) -> Vec<u8> {
		let mut out = abi_word(0x20).to_vec();
		out.extend_from_slice(&abi_word(value.len() as u64));
		out.extend_from_slice(value.as_bytes());
		out.resize(out.len().div_ceil(32) * 32, 0);
		out
	}

	fn address_word(byte: u8) -> Vec<u8> {
		let mut word = [0u8; 32];
		word[12..].copy_from_slice(&[byte; 20]);
		word.to_vec()
	}

	fn reverted() -> Result<Vec<u8>, DotnsViewError> {
		Err(DotnsViewError::Reverted(DotnsContractError::Reverted {
			detail: "(empty)".to_string(),
		}))
	}

	/// A dotNS where the controller is `0xcc`, the registry `0xd0` and every name resolves
	/// through `resolver`; `text` answers as scripted and records the node it was asked for.
	struct ScriptedName {
		resolver: fn() -> Result<Vec<u8>, DotnsViewError>,
		text: fn() -> Result<Vec<u8>, DotnsViewError>,
		asked_node: Option<[u8; 32]>,
	}

	impl ScriptedName {
		fn serving(text: fn() -> Result<Vec<u8>, DotnsViewError>) -> Self {
			Self { resolver: || Ok(address_word(0xe0)), text, asked_node: None }
		}
	}

	#[crate::platform::async_trait]
	impl DotnsTransport for ScriptedName {
		async fn storage(&mut self, _key: Vec<u8>) -> Result<Option<Vec<u8>>, DotnsError> {
			Ok(Some([0xcc; 20].to_vec()))
		}

		async fn view(
			&mut self,
			_dest: &[u8; 20],
			input: Vec<u8>,
		) -> Result<Vec<u8>, DotnsViewError> {
			let sel: [u8; 4] = input[..4].try_into().expect("selector prefix; qed");
			if sel == selector("protocolRegistry()") {
				Ok(address_word(0xa1))
			} else if sel == selector("tld()") {
				Ok(abi_string(".paseo"))
			} else if sel == selector("get(bytes32)") {
				Ok(address_word(0xd0))
			} else if sel == selector("resolver(bytes32)") {
				(self.resolver)()
			} else if sel == selector("text(bytes32,string)") {
				self.asked_node = Some(input[4..36].try_into().expect("node word; qed"));
				(self.text)()
			} else {
				panic!("unscripted view {}", hex::encode(sel));
			}
		}
	}

	fn read(
		transport: &mut ScriptedName,
		label: &str,
	) -> Result<Option<ProfileRecord>, DotnsError> {
		futures::executor::block_on(read_record(transport, label))
	}

	#[test]
	fn a_lite_name_is_two_levels_under_the_tld_as_the_writer_hashed_it() {
		// The nodes seity wrote under on 26 October 2026 (app/src/chain/dotns.ts nameNode).
		assert_eq!(
			hex::encode(name_node("paseo", "desktop.57")),
			"ce26105415899e583bec51e579e37dac32b1637c21e9ceea4a56e47620fcc9da"
		);
		assert_eq!(
			hex::encode(name_node("paseo", "scarcityconsole.41")),
			"3bd7db3fcc99d00c648b5a75e46c218f87476dbbb4b75cfc9e2167bd1930123f"
		);
		// A full name is one level, which is also what a product manifest hashes.
		assert_eq!(name_node("paseo", "alice"), namehash_under(&tld_node("paseo"), "alice"));
		assert_ne!(
			name_node("paseo", "desktop.57"),
			namehash_under(&tld_node("paseo"), "desktop.57")
		);
	}

	#[test]
	fn reads_the_record_the_writer_published() {
		let mut transport = ScriptedName::serving(|| Ok(abi_string(VECTOR_BASE64)));
		let record = read(&mut transport, "desktop.57").expect("walk").expect("a record");
		assert_eq!(record.avatar().map(|a| a.cid), Some(VECTOR_CID.to_string()));
		assert_eq!(record.avatar().map(|a| a.format), Some(ProfileAvatarFormat::Webp));
		assert_eq!(record.name(), Some("Alice".to_string()));
		assert_eq!(transport.asked_node, Some(name_node("paseo", "desktop.57")));
	}

	#[test]
	fn a_trailing_tld_is_tolerated_and_the_node_is_the_same() {
		let mut transport = ScriptedName::serving(|| Ok(abi_string(VECTOR_BASE64)));
		read(&mut transport, "desktop.57.paseo").expect("walk").expect("a record");
		assert_eq!(transport.asked_node, Some(name_node("paseo", "desktop.57")));
	}

	#[test]
	fn no_record_is_none_in_every_shape_it_takes() {
		// A name still on the reverse resolver: `text` reverts.
		let mut on_reverse = ScriptedName::serving(reverted);
		assert_eq!(read(&mut on_reverse, "alice").expect("walk"), None);
		// No resolver at all.
		let mut unresolved = ScriptedName {
			resolver: || Ok(address_word(0)),
			text: || panic!("must not read"),
			asked_node: None,
		};
		assert_eq!(read(&mut unresolved, "alice").expect("walk"), None);
		// An empty record, and a record that is not a profile.
		let mut empty = ScriptedName::serving(|| Ok(abi_string("")));
		assert_eq!(read(&mut empty, "alice").expect("walk"), None);
		let mut junk = ScriptedName::serving(|| Ok(abi_string("not base64!")));
		assert_eq!(read(&mut junk, "alice").expect("walk"), None);
		let mut not_a_record = ScriptedName::serving(|| Ok(abi_string("AAAA")));
		assert_eq!(read(&mut not_a_record, "alice").expect("walk"), None);
	}

	#[test]
	fn a_chain_failure_is_an_error_not_an_absence() {
		let mut failing =
			ScriptedName::serving(|| Err(DotnsViewError::Failed("node unreachable".into())));
		assert_eq!(
			read(&mut failing, "alice").unwrap_err(),
			DotnsError::TextRecord {
				key: "profile".to_string(),
				source: DotnsCallError::View(DotnsViewError::Failed("node unreachable".into())),
			}
		);
	}

	#[test]
	fn avatar_bytes_are_kept_only_when_they_hash_to_the_cid() {
		let bytes = b"not really an image".to_vec();
		let cid = preimage_cid(&preimage_key(&bytes));

		let mut platform =
			StubPlatform { preimage_lookup_value: Some(bytes.clone()), ..StubPlatform::default() };
		assert_eq!(
			futures::executor::block_on(fetch_avatar_bytes(&platform, &cid)),
			Some(bytes.clone())
		);

		platform.preimage_lookup_value = Some(b"forged".to_vec());
		assert_eq!(futures::executor::block_on(fetch_avatar_bytes(&platform, &cid)), None);

		platform.preimage_lookup_value = None;
		assert_eq!(futures::executor::block_on(fetch_avatar_bytes(&platform, &cid)), None);

		platform.preimage_lookup_value = Some(bytes);
		assert_eq!(
			futures::executor::block_on(fetch_avatar_bytes(&platform, "QmNotABulletinCid")),
			None
		);
	}

	#[test]
	fn the_cache_is_honoured_for_an_hour_and_a_miss_is_cached_too() {
		let platform = Arc::new(StubPlatform::default());
		let key = CoreStorageKey::public_profile("Alice.42");
		assert_eq!(key, CoreStorageKey::PublicProfile { label: "alice.42".to_string() });
		let now = 1_800_000_000;
		let miss = CachedProfile { fetched_at_secs: now, record: None };
		futures::executor::block_on(platform.write_core_storage(key.clone(), miss.encode()))
			.unwrap();

		let fresh = futures::executor::block_on(cached(
			platform.as_ref(),
			&key,
			now + PUBLIC_PROFILE_TTL_SECS - 1,
		));
		assert_eq!(fresh, Some(miss));
		let stale = futures::executor::block_on(cached(
			platform.as_ref(),
			&key,
			now + PUBLIC_PROFILE_TTL_SECS,
		));
		assert_eq!(stale, None);
		let from_the_future = futures::executor::block_on(cached(platform.as_ref(), &key, now - 1));
		assert_eq!(from_the_future, None, "a stamp ahead of the clock is not honoured");
	}

	#[test]
	fn base64_is_the_standard_padded_form_and_nothing_else() {
		let decoded = decode_base64(VECTOR_BASE64).expect("the writer's text decodes");
		assert_eq!(
			hex::encode(&decoded),
			"08186176617461720101f86261666b32627a616365637632786b356c766f7632786b356c766f7632786b356c766f7632786b356c766f7632786b356c766f7632786b356c766f76327702106e616d6514416c696365"
		);
		assert_eq!(decode_base64("").as_deref(), Some(&[][..]));
		assert_eq!(decode_base64("QQ==").as_deref(), Some(&b"A"[..]));
		assert_eq!(decode_base64("QUI=").as_deref(), Some(&b"AB"[..]));
		assert_eq!(decode_base64("QUJD").as_deref(), Some(&b"ABC"[..]));
		for bad in [
			"QQ",
			"QQ=",
			"Q===",
			"QUJD=",
			"QU-J",
			"not base64!",
			"QR==", // dirty padding bits
		] {
			assert_eq!(decode_base64(bad), None, "{bad:?}");
		}
	}

	#[test]
	fn labels_a_host_may_pass() {
		for ok in ["alice", "alice.42", "scarcity-console.07", "a.b.c"] {
			assert!(acceptable_label(ok), "{ok}");
		}
		for bad in
			["", "Alice", "alice..42", ".alice", "alice.", "al ice", "alice_42", &"a".repeat(65)]
		{
			assert!(!acceptable_label(bad), "{bad:?}");
		}
	}
}
