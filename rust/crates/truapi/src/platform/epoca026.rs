//! Host-only key migration from Epoca's pinned 0.26 core.
//!
//! The private SCALE types below freeze `platform.rs`, `product_id.rs`,
//! `v01/permissions.rs`, and `v01/account.rs` at
//! ee768f1914c91dea9f5bfdbe185e2c4993f960e5. They must
//! not use the current permission or storage enums to decode installed keys.
//! Values, including inner encrypted payloads, never enter this module.

use parity_scale_codec::{Decode, DecodeAll, Encode};

use super::{CoreStorageKey, CoreStorageKeyDescription, ProductId, describe_core_storage_key};
use crate::dotns_name::is_dns_label;

/// Metadata returned by the pinned host for authenticating the old envelope.
///
/// Unlike current ownership metadata, this preserves the exact stored string
/// for string-owned slots. In particular, the pinned host described
/// `NativeChatDevice` as wallet-owned even though its key includes a product.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epoca026CoreStorageKeyDescription {
	/// Stable slot kind in the old authenticated envelope.
	pub kind: &'static str,
	/// Exact old envelope owner, absent for wallet-owned slots.
	pub product_id: Option<String>,
}

/// A key-only migration, with both sides of the envelope authentication boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Epoca026CoreStorageKeyMigration {
	/// Current SCALE key. Never a permission authorization key.
	pub current_encoded_key: Vec<u8>,
	/// Descriptor to authenticate against the old key and old envelope.
	pub legacy_description: Epoca026CoreStorageKeyDescription,
	/// Descriptor to authenticate when sealing the unchanged value at the new key.
	pub current_description: CoreStorageKeyDescription,
}

/// A legacy key that cannot safely be imported into the current store.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum Epoca026CoreStorageKeyMigrationError {
	/// Malformed, unknown, truncated, or trailing SCALE bytes.
	#[display("invalid Epoca 0.26 core storage key encoding")]
	InvalidEncoding,
	/// Encoding or owner spelling would change on a canonical round trip.
	#[display("noncanonical Epoca 0.26 core storage key encoding")]
	NonCanonicalEncoding,
	/// A product owner is not accepted by the current product identity policy.
	#[display("invalid Epoca 0.26 core storage product identifier")]
	InvalidProductId,
	/// Old consent must be archived or skipped, never imported as a live grant.
	#[display("Epoca 0.26 permission authorization is retired; fresh consent is required")]
	RetiredPermissionAuthorization,
}

/// Describe a pinned legacy key for host-owned cleanup or authentication.
///
/// This is metadata only: even a retired permission can be described, but it
/// cannot be imported by [`migrate_epoca026_core_storage_key`]. String fields
/// retain the old spelling exactly; they are not current product identities.
/// Never use this decoder on keys from the current storage namespace.
pub fn describe_epoca026_core_storage_key(
	encoded: &[u8],
) -> Result<Epoca026CoreStorageKeyDescription, Epoca026CoreStorageKeyMigrationError> {
	let (legacy, _) = decode_legacy_core_storage_key(encoded)?;
	Ok(legacy.description())
}

/// Strictly translate one key from the pinned Epoca 0.26 store.
///
/// Call this only for keys from that explicitly identified source namespace,
/// never as a fallback for failed current-key decoding. Authenticate the old
/// envelope using `legacy_description` and the original bytes before storing
/// the unchanged value with `current_description` and `current_encoded_key`.
/// Hosts must handle destination collisions without overwriting another value:
/// full manifest ids can converge onto the same shared label.
///
/// All legacy permission payloads are decoded with their original codecs, but
/// always return `RetiredPermissionAuthorization`, including denials. No old
/// consent is carried into the current authorization namespace.
pub fn migrate_epoca026_core_storage_key(
	encoded: &[u8],
) -> Result<Epoca026CoreStorageKeyMigration, Epoca026CoreStorageKeyMigrationError> {
	let (legacy, mut current_encoded_key) = decode_legacy_core_storage_key(encoded)?;
	if matches!(legacy, LegacyCoreStorageKey::PermissionAuthorization { .. }) {
		return Err(Epoca026CoreStorageKeyMigrationError::RetiredPermissionAuthorization);
	}
	let legacy_description = legacy.description();
	let current = legacy.into_current()?;
	current_encoded_key.clear();
	current.encode_to(&mut current_encoded_key);
	let current_description = describe_core_storage_key(&current_encoded_key)
		.map_err(|_| Epoca026CoreStorageKeyMigrationError::InvalidProductId)?;
	Ok(Epoca026CoreStorageKeyMigration {
		current_encoded_key,
		legacy_description,
		current_description,
	})
}

fn decode_legacy_core_storage_key(
	encoded: &[u8],
) -> Result<(LegacyCoreStorageKey, Vec<u8>), Epoca026CoreStorageKeyMigrationError> {
	let legacy = LegacyCoreStorageKey::decode_all(&mut &*encoded)
		.map_err(|_| Epoca026CoreStorageKeyMigrationError::InvalidEncoding)?;
	let canonical = legacy.encode();
	if canonical != encoded {
		return Err(Epoca026CoreStorageKeyMigrationError::NonCanonicalEncoding);
	}
	Ok((legacy, canonical))
}

fn canonical_product_id(value: &str) -> Result<ProductId, Epoca026CoreStorageKeyMigrationError> {
	let product_id = value
		.parse::<ProductId>()
		.map_err(|_| Epoca026CoreStorageKeyMigrationError::InvalidProductId)?;
	if product_id.as_str() != value {
		// Normalizing the key alone could change the identity authenticated by
		// an untouched encrypted value. Reject rather than silently rebind it.
		return Err(Epoca026CoreStorageKeyMigrationError::NonCanonicalEncoding);
	}
	Ok(product_id)
}

// The one typed ProductId in the pinned key schema normalized on decode.
// Keep that grammar frozen too: current local-artifact identities did not
// exist in it. The round-trip comparison rejects normalization-changing input.
#[derive(Encode)]
struct LegacyProductId(String);

impl Decode for LegacyProductId {
	fn decode<I: parity_scale_codec::Input>(
		input: &mut I,
	) -> Result<Self, parity_scale_codec::Error> {
		let raw = String::decode(input)?;
		let normalized = raw.trim().to_ascii_lowercase();
		if normalized.is_empty() || normalized.len() > 256 {
			return Err("invalid pinned product identifier length".into());
		}
		let localhost = normalized == "localhost" ||
			normalized.strip_prefix("localhost:").is_some_and(|port| {
				!port.is_empty() && port.bytes().all(|byte| byte.is_ascii_digit())
			});
		let dotns = normalized.split('.').all(|label| {
			!label.is_empty() &&
				label.len() <= 63 &&
				!label.starts_with('-') &&
				!label.ends_with('-') &&
				label.bytes().all(|byte| {
					byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
				})
		}) && normalized.rsplit_once('.').is_some_and(|(label, tld)| {
			label != "localhost" && matches!(tld, "dot" | "paseo" | "testnet")
		});
		if !localhost && !dotns {
			return Err("invalid pinned product identifier".into());
		}
		Ok(Self(normalized))
	}
}

// Every index is explicit, including the pinned enum's originally implicit
// indices. No current permission or key decoder participates in this schema.
#[derive(Encode, Decode)]
enum LegacyCoreStorageKey {
	#[codec(index = 0)]
	AuthSession,
	#[codec(index = 1)]
	PairingDeviceIdentity,
	#[codec(index = 2)]
	PermissionAuthorization { product_id: String, request: LegacyPermissionAuthorizationRequest },
	#[codec(index = 3)]
	AllowanceKeys { session_id: String },
	#[codec(index = 4)]
	LastProcessedPairingStatement,
	#[codec(index = 5)]
	AutoSigningKey { product_id: String },
	#[codec(index = 6)]
	AutoSigningKeys,
	#[codec(index = 7)]
	RingVrfRegistry { root_public_key: [u8; 32] },
	#[codec(index = 8)]
	StatementRenewalTargets,
	#[codec(index = 9)]
	DeviceEncryptionKey,
	#[codec(index = 10)]
	ProductSubtree { session_id: String, product_id: LegacyProductId },
	#[codec(index = 11)]
	SsoResponderRequestLedger {
		root_public_key: [u8; 32],
		peer_statement_account_id: [u8; 32],
		peer_encryption_public_key: [u8; 32],
	},
	#[codec(index = 12)]
	ProductManifest { product_id: String },
	#[codec(index = 13)]
	MainPurseCoinage { root_public_key: [u8; 32], genesis_hash: [u8; 32] },
	#[codec(index = 14)]
	NativeChatDevice { root_public_key: [u8; 32], genesis_hash: [u8; 32], product_id: String },
	#[codec(index = 15)]
	NativeChatFileChunk {
		root_public_key: [u8; 32],
		genesis_hash: [u8; 32],
		product_id: String,
		attachment_id: [u8; 32],
		chunk_index: u32,
	},
	#[codec(index = 16)]
	NativeChatProducts { root_public_key: [u8; 32], genesis_hash: [u8; 32] },
	#[codec(index = 17)]
	ProfileDisclosure { root_public_key: [u8; 32], genesis_hash: [u8; 32] },
	#[codec(index = 18)]
	ProfileReferencesReceived {
		root_public_key: [u8; 32],
		genesis_hash: [u8; 32],
		product_id: String,
	},
	#[codec(index = 19)]
	ProfilePersonalReferencesReceived { root_public_key: [u8; 32], genesis_hash: [u8; 32] },
	#[codec(index = 20)]
	NotificationReceiving,
	#[codec(index = 21)]
	FundingSessions,
	#[codec(index = 22)]
	FundingSessionsUnreadable,
	#[codec(index = 23)]
	FundingSessionPack,
}

impl LegacyCoreStorageKey {
	fn description(&self) -> Epoca026CoreStorageKeyDescription {
		let (kind, product_id) = match self {
			Self::AuthSession => ("AuthSession", None),
			Self::PairingDeviceIdentity => ("PairingDeviceIdentity", None),
			Self::PermissionAuthorization { product_id, .. } => {
				("PermissionAuthorization", Some(product_id.clone()))
			},
			Self::AllowanceKeys { .. } => ("AllowanceKeys", None),
			Self::LastProcessedPairingStatement => ("LastProcessedPairingStatement", None),
			Self::AutoSigningKey { product_id } => ("AutoSigningKey", Some(product_id.clone())),
			Self::AutoSigningKeys => ("AutoSigningKeys", None),
			Self::RingVrfRegistry { .. } => ("RingVrfRegistry", None),
			Self::StatementRenewalTargets => ("StatementRenewalTargets", None),
			Self::DeviceEncryptionKey => ("DeviceEncryptionKey", None),
			Self::ProductSubtree { product_id, .. } => {
				("ProductSubtree", Some(product_id.0.clone()))
			},
			Self::SsoResponderRequestLedger { .. } => ("SsoResponderRequestLedger", None),
			Self::ProductManifest { product_id } => ("ProductManifest", Some(product_id.clone())),
			Self::MainPurseCoinage { .. } => ("MainPurseCoinage", None),
			Self::NativeChatDevice { .. } => ("NativeChatDevice", None),
			Self::NativeChatFileChunk { product_id, .. } => {
				("NativeChatFileChunk", Some(product_id.clone()))
			},
			Self::NativeChatProducts { .. } => ("NativeChatProducts", None),
			Self::ProfileDisclosure { .. } => ("ProfileDisclosure", None),
			Self::ProfileReferencesReceived { product_id, .. } => {
				("ProfileReferencesReceived", Some(product_id.clone()))
			},
			Self::ProfilePersonalReferencesReceived { .. } => {
				("ProfilePersonalReferencesReceived", None)
			},
			Self::NotificationReceiving => ("NotificationReceiving", None),
			Self::FundingSessions => ("FundingSessions", None),
			Self::FundingSessionsUnreadable => ("FundingSessionsUnreadable", None),
			Self::FundingSessionPack => ("FundingSessionPack", None),
		};
		Epoca026CoreStorageKeyDescription { kind, product_id }
	}

	fn into_current(self) -> Result<CoreStorageKey, Epoca026CoreStorageKeyMigrationError> {
		Ok(match self {
			Self::AuthSession => CoreStorageKey::AuthSession,
			Self::PairingDeviceIdentity => CoreStorageKey::PairingDeviceIdentity,
			Self::PermissionAuthorization { .. } => {
				return Err(Epoca026CoreStorageKeyMigrationError::RetiredPermissionAuthorization);
			},
			Self::AllowanceKeys { session_id } => CoreStorageKey::AllowanceKeys { session_id },
			Self::LastProcessedPairingStatement => CoreStorageKey::LastProcessedPairingStatement,
			Self::AutoSigningKey { product_id } => {
				CoreStorageKey::AutoSigningKey { product_id: canonical_product_id(&product_id)? }
			},
			Self::AutoSigningKeys => CoreStorageKey::AutoSigningKeys,
			Self::RingVrfRegistry { root_public_key } => {
				CoreStorageKey::RingVrfRegistry { root_public_key }
			},
			Self::StatementRenewalTargets => CoreStorageKey::StatementRenewalTargets,
			Self::DeviceEncryptionKey => CoreStorageKey::DeviceEncryptionKey,
			Self::ProductSubtree { session_id, product_id } => CoreStorageKey::ProductSubtree {
				session_id,
				product_id: canonical_product_id(&product_id.0)?,
			},
			Self::SsoResponderRequestLedger {
				root_public_key,
				peer_statement_account_id,
				peer_encryption_public_key,
			} => CoreStorageKey::SsoResponderRequestLedger {
				root_public_key,
				peer_statement_account_id,
				peer_encryption_public_key,
			},
			Self::ProductManifest { product_id } => {
				// The pinned helper wrote bare labels, while direct callers could
				// write the full id described by the old field documentation.
				let product_label = if is_dns_label(&product_id) {
					product_id
				} else {
					canonical_product_id(&product_id)?.bare_label().to_string()
				};
				CoreStorageKey::ProductManifest { product_label }
			},
			Self::MainPurseCoinage { root_public_key, genesis_hash } => {
				CoreStorageKey::MainPurseCoinage { root_public_key, genesis_hash }
			},
			Self::NativeChatDevice { root_public_key, genesis_hash, product_id } => {
				canonical_product_id(&product_id)?;
				CoreStorageKey::NativeChatDevice { root_public_key, genesis_hash, product_id }
			},
			Self::NativeChatFileChunk {
				root_public_key,
				genesis_hash,
				product_id,
				attachment_id,
				chunk_index,
			} => {
				canonical_product_id(&product_id)?;
				CoreStorageKey::NativeChatFileChunk {
					root_public_key,
					genesis_hash,
					product_id,
					attachment_id,
					chunk_index,
				}
			},
			Self::NativeChatProducts { root_public_key, genesis_hash } => {
				CoreStorageKey::NativeChatProducts { root_public_key, genesis_hash }
			},
			Self::ProfileDisclosure { root_public_key, genesis_hash } => {
				CoreStorageKey::ProfileDisclosure { root_public_key, genesis_hash }
			},
			Self::ProfileReferencesReceived { root_public_key, genesis_hash, product_id } => {
				canonical_product_id(&product_id)?;
				CoreStorageKey::ProfileReferencesReceived {
					root_public_key,
					genesis_hash,
					product_id,
				}
			},
			Self::ProfilePersonalReferencesReceived { root_public_key, genesis_hash } => {
				CoreStorageKey::ProfilePersonalReferencesReceived { root_public_key, genesis_hash }
			},
			Self::NotificationReceiving => CoreStorageKey::NotificationReceiving,
			Self::FundingSessions => CoreStorageKey::FundingSessions,
			Self::FundingSessionsUnreadable => CoreStorageKey::FundingSessionsUnreadable,
			Self::FundingSessionPack => CoreStorageKey::FundingSessionPack,
		})
	}
}

// These are the STORED permission tags, not ee768f191's public wire tags.
// In particular, stored remote 5 is JAM (with a 32-byte genesis), 6 is balance,
// and stored authorization 4 is Chat, not a cross-product signature.
#[derive(Encode, Decode)]
enum LegacyPermissionAuthorizationRequest {
	#[codec(index = 0)]
	Device(LegacyDevicePermission),
	#[codec(index = 1)]
	Remote(LegacyRemotePermissionRequest),
	#[codec(index = 2)]
	IdentityDisclosure,
	#[codec(index = 3)]
	AccountAccess { target_product_id: String },
	#[codec(index = 4)]
	ChatAuthority,
	#[codec(index = 5)]
	StatementStoreAllowance { derivation_index: Option<LegacyDerivationIndex> },
	#[codec(index = 6)]
	ProfileDisclosure,
	#[codec(index = 7)]
	CrossProductSignature { target_product_id: String, kind: LegacyCrossProductSignatureKind },
}

#[derive(Encode, Decode)]
enum LegacyDevicePermission {
	#[codec(index = 0)]
	Notifications,
	#[codec(index = 1)]
	Camera,
	#[codec(index = 2)]
	Microphone,
	#[codec(index = 3)]
	Bluetooth,
	#[codec(index = 4)]
	Nfc,
	#[codec(index = 5)]
	Location,
	#[codec(index = 6)]
	Clipboard,
	#[codec(index = 7)]
	OpenUrl,
	#[codec(index = 8)]
	Biometrics,
	#[codec(index = 9)]
	Motion,
}

#[derive(Encode, Decode)]
struct LegacyRemotePermissionRequest {
	permission: LegacyRemotePermission,
}

#[derive(Encode, Decode)]
enum LegacyRemotePermission {
	#[codec(index = 0)]
	Remote { domains: Vec<String> },
	#[codec(index = 1)]
	WebRtc,
	#[codec(index = 2)]
	ChainSubmit,
	#[codec(index = 3)]
	PreimageSubmit,
	#[codec(index = 4)]
	StatementSubmit,
	#[codec(index = 5)]
	JamPeers { genesis: [u8; 32] },
	#[codec(index = 6)]
	BalanceAccess,
}

#[derive(Encode, Decode)]
enum LegacyDerivationIndex {
	#[codec(index = 0)]
	Index(u32),
	#[codec(index = 1)]
	Raw([u8; 32]),
}

#[derive(Encode, Decode)]
enum LegacyCrossProductSignatureKind {
	#[codec(index = 0)]
	Payload,
	#[codec(index = 1)]
	Transaction,
	#[codec(index = 2)]
	Raw,
	#[codec(index = 3)]
	StatementProof,
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::platform::CoreStorageKeyOwner;

	// Fixtures deliberately use literal pinned bytes, never the migration
	// enums' encoders, so changing a frozen discriminant cannot fix its test.
	fn short_string(bytes: &mut Vec<u8>, value: &str) {
		assert!(value.len() < 64);
		bytes.push((value.len() as u8) << 2);
		bytes.extend_from_slice(value.as_bytes());
	}

	fn owned_key(tag: u8, owner: &str) -> Vec<u8> {
		let mut bytes = vec![tag];
		short_string(&mut bytes, owner);
		bytes
	}

	fn wallet_key(tag: u8) -> Vec<u8> {
		let mut bytes = vec![tag];
		bytes.extend_from_slice(&[0x11; 32]);
		bytes.extend_from_slice(&[0x22; 32]);
		bytes
	}

	fn chat_key(tag: u8, owner: &str) -> Vec<u8> {
		let mut bytes = wallet_key(tag);
		short_string(&mut bytes, owner);
		if tag == 15 {
			bytes.extend_from_slice(&[0x33; 32]);
			bytes.extend_from_slice(&[0x78, 0x56, 0x34, 0x12]);
		}
		bytes
	}

	fn assert_migration(
		old: &[u8],
		new: &[u8],
		kind: &'static str,
		legacy_owner: Option<&str>,
		owner: Option<CoreStorageKeyOwner>,
	) {
		let migrated = migrate_epoca026_core_storage_key(old).expect("valid pinned key");
		assert_eq!(migrated.current_encoded_key, new);
		assert_eq!(
			migrated.legacy_description,
			Epoca026CoreStorageKeyDescription { kind, product_id: legacy_owner.map(str::to_owned) }
		);
		assert_eq!(migrated.current_description, CoreStorageKeyDescription { kind, owner });
	}

	fn product_owner(id: &str) -> Option<CoreStorageKeyOwner> {
		Some(CoreStorageKeyOwner::Product(ProductId::for_test(id)))
	}

	#[test]
	fn common_wallet_keys_preserve_exact_bytes_and_descriptors() {
		for (tag, kind) in [
			(0, "AuthSession"),
			(1, "PairingDeviceIdentity"),
			(4, "LastProcessedPairingStatement"),
			(6, "AutoSigningKeys"),
			(8, "StatementRenewalTargets"),
			(9, "DeviceEncryptionKey"),
			(20, "NotificationReceiving"),
			(21, "FundingSessions"),
			(22, "FundingSessionsUnreadable"),
			(23, "FundingSessionPack"),
		] {
			assert_migration(&[tag], &[tag], kind, None, None);
		}
		let allowance = [3, 4, b's'];
		assert_migration(&allowance, &allowance, "AllowanceKeys", None, None);
		let mut registry = vec![7];
		registry.extend_from_slice(&[0x11; 32]);
		assert_migration(&registry, &registry, "RingVrfRegistry", None, None);
		let mut ledger = wallet_key(11);
		ledger.extend_from_slice(&[0x33; 32]);
		assert_migration(&ledger, &ledger, "SsoResponderRequestLedger", None, None);
		for (tag, kind) in [
			(13, "MainPurseCoinage"),
			(16, "NativeChatProducts"),
			(17, "ProfileDisclosure"),
			(19, "ProfilePersonalReferencesReceived"),
		] {
			let key = wallet_key(tag);
			assert_migration(&key, &key, kind, None, None);
		}
	}

	#[test]
	fn product_keys_preserve_full_executable_identity() {
		for id in ["app.echat.paseo", "echat.dot", "localhost:3000"] {
			let auto = owned_key(5, id);
			assert_migration(&auto, &auto, "AutoSigningKey", Some(id), product_owner(id));
			let mut subtree = vec![10, 4, b's'];
			short_string(&mut subtree, id);
			assert_migration(&subtree, &subtree, "ProductSubtree", Some(id), product_owner(id));
			let references = chat_key(18, id);
			assert_migration(
				&references,
				&references,
				"ProfileReferencesReceived",
				Some(id),
				product_owner(id),
			);
		}
	}

	#[test]
	fn chat_slots_move_without_rewriting_fields_or_legacy_ownership() {
		let id = "echat.paseo";
		let device = chat_key(14, id);
		let mut current_device = device.clone();
		current_device[0] = 64;
		assert_migration(&device, &current_device, "NativeChatDevice", None, None);
		let chunk = chat_key(15, id);
		let mut current_chunk = chunk.clone();
		current_chunk[0] = 65;
		assert_migration(
			&chunk,
			&current_chunk,
			"NativeChatFileChunk",
			Some(id),
			product_owner(id),
		);
		let decoded = CoreStorageKey::decode_all(&mut current_chunk.as_slice()).unwrap();
		assert_eq!(
			decoded,
			CoreStorageKey::NativeChatFileChunk {
				root_public_key: [0x11; 32],
				genesis_hash: [0x22; 32],
				product_id: id.to_string(),
				attachment_id: [0x33; 32],
				chunk_index: 0x12345678,
			}
		);
	}

	#[test]
	fn manifests_keep_old_authenticated_owner_and_use_current_bare_label() {
		for (old_owner, label) in [
			("echat", "echat"),
			("echat.paseo", "echat"),
			("app.echat.paseo", "echat"),
			("echat.attacker.dot", "attacker"),
			("localhost", "localhost"),
			("localhost:3000", "localhost:3000"),
		] {
			assert_migration(
				&owned_key(12, old_owner),
				&owned_key(12, label),
				"ProductManifest",
				Some(old_owner),
				Some(CoreStorageKeyOwner::ProductLabel(label.to_string())),
			);
		}
	}

	fn permission(payload: &[u8]) -> Vec<u8> {
		let mut bytes = owned_key(2, "echat.paseo");
		bytes.extend_from_slice(payload);
		bytes
	}

	#[test]
	fn all_pinned_permission_shapes_are_retired_not_current_grants() {
		let mut payloads = vec![
			vec![2],                               // IdentityDisclosure
			vec![4],                               // ChatAuthority, not CrossProductSignature
			vec![5, 0],                            // StatementStoreAllowance(None)
			vec![5, 1, 0, 0x78, 0x56, 0x34, 0x12], // Some(Index)
			vec![6],                               // ProfileDisclosure
			vec![1, 0, 4, 4, b'*'],                // Remote with one domain
		];
		payloads.extend((0..=9).map(|device| vec![0, device]));
		payloads.extend((1..=4).chain([6]).map(|remote| vec![1, remote]));
		let mut jam = vec![1, 5];
		jam.extend_from_slice(&[0x44; 32]);
		payloads.push(jam);
		let mut raw_index = vec![5, 1, 1];
		raw_index.extend_from_slice(&[0x55; 32]);
		payloads.push(raw_index);
		payloads.push(owned_key(3, "other"));
		for kind in 0..=3 {
			let mut signature = owned_key(7, "other");
			signature.push(kind);
			payloads.push(signature);
		}
		for payload in payloads {
			assert_eq!(
				migrate_epoca026_core_storage_key(&permission(&payload)),
				Err(Epoca026CoreStorageKeyMigrationError::RetiredPermissionAuthorization),
				"pinned permission payload {payload:?}",
			);
		}
		// Old manifest-scoped decisions also remain retired, not live consent.
		assert_eq!(
			migrate_epoca026_core_storage_key(&[2, 4, b'x', 2]),
			Err(Epoca026CoreStorageKeyMigrationError::RetiredPermissionAuthorization),
		);
	}

	#[test]
	fn malformed_permission_payloads_are_not_treated_as_valid_archival_keys() {
		for payload in [
			vec![],
			vec![0, 10],
			vec![1, 5],
			vec![1, 7],
			vec![1, 111],
			vec![1, 112],
			vec![5, 2],
			vec![5, 1, 2],
			vec![7],
			vec![8],
			vec![4, 0],
			vec![1, 6, 0],
		] {
			assert_eq!(
				migrate_epoca026_core_storage_key(&permission(&payload)),
				Err(Epoca026CoreStorageKeyMigrationError::InvalidEncoding),
			);
		}
		let mut invalid_signature = owned_key(7, "other");
		invalid_signature.push(4);
		assert!(migrate_epoca026_core_storage_key(&permission(&invalid_signature)).is_err());
	}

	#[test]
	fn rejects_unknown_truncated_trailing_and_noncanonical_scale() {
		for bytes in [
			vec![],
			vec![24],
			vec![64],
			vec![65],
			vec![128],
			vec![255],
			vec![0, 0],
			vec![14],
			vec![15],
			vec![5, 4, 0xff],
			vec![3, 5, 0, b's'], // nonminimal compact string length
			vec![3, 2, 0, 0, 0], // nonminimal four-byte compact zero
		] {
			assert!(migrate_epoca026_core_storage_key(&bytes).is_err(), "{bytes:?}");
		}
		let complete = chat_key(15, "echat.paseo");
		for len in 0..complete.len() {
			assert!(migrate_epoca026_core_storage_key(&complete[..len]).is_err());
		}
		let mut trailing = complete;
		trailing.push(0);
		assert!(migrate_epoca026_core_storage_key(&trailing).is_err());
		let noncanonical_domain_count = [1, 0, 5, 0, 4, b'*'];
		assert!(
			migrate_epoca026_core_storage_key(&permission(&noncanonical_domain_count)).is_err()
		);
	}

	#[test]
	fn rejects_owner_rebinding_and_invalid_current_product_identities() {
		for owner in ["ECHat.paseo", " echat.paseo ", "echat", "evil.com", "../echat.paseo", ""] {
			assert!(migrate_epoca026_core_storage_key(&owned_key(5, owner)).is_err());
			let mut subtree = vec![10, 4, b's'];
			short_string(&mut subtree, owner);
			assert!(migrate_epoca026_core_storage_key(&subtree).is_err());
			for tag in [14, 15, 18] {
				assert!(migrate_epoca026_core_storage_key(&chat_key(tag, owner)).is_err());
			}
		}
		for owner in ["ECHat.paseo", " echat ", "evil.com", "../echat.paseo", ""] {
			assert!(migrate_epoca026_core_storage_key(&owned_key(12, owner)).is_err());
		}
	}

	#[test]
	fn legacy_description_can_identify_retired_consent_without_migrating_it() {
		let key = permission(&[4]);
		assert_eq!(
			describe_epoca026_core_storage_key(&key),
			Ok(Epoca026CoreStorageKeyDescription {
				kind: "PermissionAuthorization",
				product_id: Some("echat.paseo".to_string()),
			}),
		);
		assert_eq!(
			migrate_epoca026_core_storage_key(&key),
			Err(Epoca026CoreStorageKeyMigrationError::RetiredPermissionAuthorization),
		);
		assert!(describe_epoca026_core_storage_key(&permission(&[1, 5])).is_err());
		assert!(describe_epoca026_core_storage_key(&[0, 0]).is_err());
		assert!(describe_epoca026_core_storage_key(&[3, 5, 0, b's']).is_err());
	}

	#[test]
	fn legacy_description_keeps_string_owners_but_freezes_typed_product_ids() {
		let string_key = owned_key(5, " ECHAT.PASEO ");
		assert_eq!(
			describe_epoca026_core_storage_key(&string_key),
			Ok(Epoca026CoreStorageKeyDescription {
				kind: "AutoSigningKey",
				product_id: Some(" ECHAT.PASEO ".to_string()),
			}),
		);
		assert!(migrate_epoca026_core_storage_key(&string_key).is_err());
		for owner in
			[" ECHAT.PASEO ", "01234567-89ab-cdef-0123-456789abcdef.epoca-local", "localhost.dot"]
		{
			let mut subtree = vec![10, 4, b's'];
			short_string(&mut subtree, owner);
			assert!(describe_epoca026_core_storage_key(&subtree).is_err());
		}
	}
}
