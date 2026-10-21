//! Capability traits a TrUAPI host must implement.
//!
//! Each trait covers a single OS-primitive surface the Rust core cannot reach
//! from its own process (key-value persistence, URL launching, push
//! notifications, permission UI, chain RPC, host-selected preimage backends).
//! Account management, signing, and statement-store protocol flows live in the
//! Rust core itself and are not part of this trait set.
//!
//! Async capability traits use `async_trait` so the combined [`Platform`]
//! surface can be used as a trait object by the runtime.

use std::collections::BTreeSet;

use futures::stream::BoxStream;
use parity_scale_codec::{Decode, Encode};
use unicode_normalization::UnicodeNormalization;

pub use async_trait::async_trait;
pub use truapi_provider::{
	ProviderError,
	platform::{ChainProvider, JsonRpcConnection},
};

use truapi::Bytes32;

/// In-memory mock platform for tests and host simulators (enable the `mock` feature).
#[cfg(feature = "mock")]
pub mod mock;

use truapi::{
	latest::{
		AllocatableResource, ChainIdentifier, ChatAction, ChatActionHandler, ChatActions,
		ChatCustomMessage, ChatFeedUpdate, ChatFile, ChatInputConfig, ChatMedia, ChatMediaItem,
		ChatMediaMeta, ChatMessage, ChatMessageBody, ChatMessageContent, ChatMessageMenu,
		ChatMessageOrigin, ChatPageAnchor, ChatParticipant, ChatParticipantEntry,
		ChatParticipantsQuery, ChatProductAction, ChatReaction, ChatReactionDetail,
		ChatReactionSummary, ChatResource, ChatRichText, ChatRoomBadge, ChatRoomCall,
		ChatRoomConfig, ChatRoomEvent, ChatRoomFeatures, ChatRoomFooter, ChatRoomMetadata,
		ChatRoomPreview, ChatRoomPreviewText, CodeFormat, FundingDirection, GenericError,
		HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
		HostChatDeleteRoomRequest, HostChatListSubscribeItem, HostChatPostMessageError,
		HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
		HostChatRegisterBotRequest, HostChatRegisterBotResponse, HostChatSetRoomCallRequest,
		HostChatSetRoomFeaturesRequest, HostChatSetRoomFooterRequest,
		HostChatSetRoomMetadataRequest, HostChatSetRoomPreviewRequest, HostChatUpdateFeedError,
		HostChatUpdateFeedRequest, HostDevicePermissionRequest, HostFeatureSupportedRequest,
		HostFeatureSupportedResponse, HostFundingStatusSubscribeItem, HostLocalStorageChangeItem,
		HostLocaleSubscribeItem, HostNavigateToError, HostPaymentBalanceSubscribeError,
		HostPaymentBalanceSubscribeItem, HostPaymentError, HostPaymentRequest,
		HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
		HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
		HostPaymentTopUpStatusSubscribeItem, HostPlatform, HostPocketListSubscribeItem,
		HostPocketRemoveCardError, HostPocketRemoveCardRequest, HostPushNotificationRequest,
		HostPushNotificationResponse, HostScannerScanRequest, HostSignPayloadRequest,
		HostSignPayloadWithLegacyAccountRequest, HostSignRawRequest,
		HostSignRawWithLegacyAccountRequest, HostThemeSubscribeItem,
		HostWorkerBeginOperationResponse, HostWorkerOperationError, LegacyAccountTxPayload,
		ProductAccountId, ProductAccountTxPayload, ProductChatMessagesQueryItem,
		ProductChatMessagesQueryRequest, ProductChatOpenLinkItem, ProductChatOpenLinkRequest,
		ProductChatParticipantsQueryItem, ProductChatParticipantsQueryRequest,
		ProductChatReactionsQueryItem, ProductChatSearchQueryItem, ProductChatSearchQueryRequest,
		ProductProofContext, RemotePermission, RemotePermissionRequest, RingLocation,
	},
	v01::HostAccountSignVrfRequest,
};
use url::{Host, Url};

/// Role-neutral host identity supplied by the embedding host.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostIdentity {
	/// Host metadata.
	pub host_info: HostInfo,
	/// Platform metadata.
	pub platform_info: PlatformInfo,
}

/// Pairing-host runtime configuration supplied by the embedding host.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingHostConfig {
	/// Host identity shown to the signing host during pairing.
	///
	/// Host-spec B.1.3 defines the host metadata consumed by the signing host:
	/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/B-inter-host.md?plain=1#L48-L60>
	pub host: HostIdentity,
	/// People-chain genesis hash used for statement-store SSO.
	pub people_chain_genesis_hash: [u8; 32],
	/// Bulletin-chain genesis hash used for in-core preimage submission.
	pub bulletin_chain_genesis_hash: [u8; 32],
	/// Asset Hub genesis hash. Session usernames and product manifests are
	/// both read from the dotNS contracts deployed there, so without a usable
	/// value no manifest resolves and every cross-product `trustedProducts`
	/// grant not already cached is refused, indistinguishably from the other
	/// product having granted nothing. All-zero says this host has no Asset
	/// Hub.
	pub asset_hub_chain_genesis_hash: [u8; 32],
	/// Deeplink URI scheme used in pairing QR payloads, without `://`.
	///
	/// Host-spec L.2-L.3 define the `polkadotapp://pair` route and construction
	/// rules:
	/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/L-url-schemes.md?plain=1#L17-L33>
	pub pairing_deeplink_scheme: String,
}

/// Signing-host runtime configuration supplied by the embedding host.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningHostConfig {
	/// Host identity. Not read by the local-signing paths yet; retained for
	/// parity with [`PairingHostConfig`] and for the future signer-side SSO
	/// responder, which advertises host identity in handshake responses.
	pub host: HostIdentity,
	/// People-chain genesis hash used for statement-store product calls.
	pub people_chain_genesis_hash: [u8; 32],
	/// Bulletin-chain genesis hash used for in-core preimage submission.
	pub bulletin_chain_genesis_hash: [u8; 32],
	/// Asset Hub genesis hash the dotNS contracts are deployed on, used to
	/// resolve the product manifests that carry `trustedProducts` grants.
	///
	/// All-zero says this host has no Asset Hub, which refuses every grant not
	/// already in the manifest cache.
	pub asset_hub_chain_genesis_hash: [u8; 32],
	/// The network's dotNS TLD without the leading dot: `dot`, `paseo`,
	/// `testnet`. Every reserved RFC-0022 identity the wallet derives ends in
	/// it: the `uid.<suffix>` identity account and the `peopl.<suffix>` person
	/// ring-VRF keys. Must match the People chain's
	/// `NetworkSuffix.NetworkSuffix` value used for proof contexts.
	pub network_suffix: String,
}

/// Product identity attached to one product-facing TrUAPI connection.
///
/// A host may create multiple product runtimes from the same long-lived host
/// runtime, each with its own product context.
#[non_exhaustive]
// `Decode` is hand-written below so decoding cannot bypass the validating
// constructor. Not a doc comment: wire-type docs are emitted into the generated
// host API, and this is a Rust-side implementation note.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct ProductContext {
	/// Product identifier used for account derivation and product-scoped
	/// storage/permission namespaces.
	///
	/// Host-spec C.7 defines accepted product id forms:
	/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L109-L128>
	pub product_id: String,
	/// Trusted kind of executable attached to this connection by the host.
	pub execution_kind: ProductExecutionKind,
}

/// Trusted kind of product executable attached to a TrUAPI connection.
///
/// Mirrors the executable kinds a product manifest declares. The variants are
/// capability classes: a connection reaches an execution-gated service only
/// when its kind matches exactly, so `App` and `Widget` carry the same
/// capability and differ only in how the host presents them, and `Worker` is
/// the only kind that may serve the Chat modality.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ProductExecutionKind {
	/// Visible full-page entrypoint such as `app/index.html`.
	#[default]
	App,
	/// Visible embedded surface such as a dashboard card.
	Widget,
	/// Headless executable the host runs while a modality holds a reference
	/// to it; the only kind that may serve the Chat modality.
	Worker,
}

/// Host metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
	/// Host name.
	pub name: String,
	/// Optional absolute HTTPS host icon URL.
	pub icon: Option<String>,
	/// Optional host version.
	pub version: Option<String>,
	/// Platform category the host runs on, reported to products via
	/// `System::host_info`.
	pub platform: HostPlatform,
}

/// Platform metadata.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlatformInfo {
	/// Optional platform/browser name.
	pub kind: Option<String>,
	/// Optional platform/browser version.
	pub version: Option<String>,
}

impl HostIdentity {
	/// Build a role-neutral host identity, validating fields whose
	/// representation cannot be made invalid by Rust types alone.
	pub fn new(
		host_info: HostInfo,
		platform_info: PlatformInfo,
	) -> Result<Self, RuntimeConfigValidationError> {
		require_non_empty("host_info.name", &host_info.name)?;
		if let Some(icon) = &host_info.icon {
			let parsed = Url::parse(icon)
				.map_err(|source| RuntimeConfigValidationError::InvalidHostIcon { source })?;
			if parsed.scheme() != "https" {
				return Err(RuntimeConfigValidationError::InsecureHostIcon {
					scheme: parsed.scheme().to_string(),
				});
			}
		}
		Ok(Self { host_info, platform_info })
	}
}

impl PairingHostConfig {
	/// Build a pairing-host runtime config, validating fields whose
	/// representation cannot be made invalid by Rust types alone.
	pub fn new(
		host_info: HostInfo,
		platform_info: PlatformInfo,
		people_chain_genesis_hash: [u8; 32],
		bulletin_chain_genesis_hash: [u8; 32],
		asset_hub_chain_genesis_hash: [u8; 32],
		pairing_deeplink_scheme: String,
	) -> Result<Self, RuntimeConfigValidationError> {
		require_non_empty("pairing_deeplink_scheme", &pairing_deeplink_scheme)?;
		if pairing_deeplink_scheme.contains("://") {
			return Err(RuntimeConfigValidationError::InvalidDeeplinkScheme {
				scheme: pairing_deeplink_scheme,
			});
		}
		let config = Self {
			host: HostIdentity::new(host_info, platform_info)?,
			people_chain_genesis_hash,
			bulletin_chain_genesis_hash,
			asset_hub_chain_genesis_hash,
			pairing_deeplink_scheme,
		};
		Ok(config)
	}
}

impl SigningHostConfig {
	/// Build a signing-host runtime config, validating fields whose
	/// representation cannot be made invalid by Rust types alone.
	pub fn new(
		host_info: HostInfo,
		platform_info: PlatformInfo,
		people_chain_genesis_hash: [u8; 32],
		bulletin_chain_genesis_hash: [u8; 32],
		asset_hub_chain_genesis_hash: [u8; 32],
		network_suffix: String,
	) -> Result<Self, RuntimeConfigValidationError> {
		validate_network_suffix(&network_suffix)?;
		Ok(Self {
			host: HostIdentity::new(host_info, platform_info)?,
			people_chain_genesis_hash,
			bulletin_chain_genesis_hash,
			asset_hub_chain_genesis_hash,
			network_suffix,
		})
	}
}

fn validate_network_suffix(network_suffix: &str) -> Result<(), RuntimeConfigValidationError> {
	require_non_empty("network_suffix", network_suffix)?;
	if !DOTNS_TLDS.contains(&network_suffix) {
		return Err(RuntimeConfigValidationError::InvalidNetworkSuffix {
			network_suffix: network_suffix.to_string(),
		});
	}
	Ok(())
}

impl ProductContext {
	/// Build a product context, validating fields whose representation cannot
	/// be made invalid by Rust types alone.
	pub fn new(product_id: String) -> Result<Self, RuntimeConfigValidationError> {
		Self::new_with_execution(product_id, ProductExecutionKind::App)
	}

	/// Build a product context for a host-selected executable kind.
	pub fn new_with_execution(
		product_id: String,
		execution_kind: ProductExecutionKind,
	) -> Result<Self, RuntimeConfigValidationError> {
		Ok(Self { product_id: normalize_product_identifier(&product_id)?, execution_kind })
	}

	/// Whether this is the game product, Jollity, on any dotNS network. A
	/// subname such as `app.dim2.dot` is a different product.
	pub fn is_game_product(&self) -> bool {
		dotns_product_label(&self.product_id) == Some(GAME_PRODUCT_LABEL)
	}

	/// Whether this is the person-to-person chat product on any dotNS
	/// network. A subname such as `app.chat.dot` is a different product.
	pub fn is_person_chat_product(&self) -> bool {
		dotns_product_label(&self.product_id) == Some(PERSON_CHAT_PRODUCT_LABEL)
	}
}

/// Decoding routes through [`ProductContext::new_with_execution`] so a frame
/// off the wire cannot produce a context the constructor rejects. The runtime
/// treats a `ProductContext` as already validated (product storage keys are
/// built with `expect`), and derivation/storage scopes are keyed by
/// `product_id`, so an unnormalized id would split one logical product across
/// two scopes.
impl Decode for ProductContext {
	fn decode<I: parity_scale_codec::Input>(
		input: &mut I,
	) -> Result<Self, parity_scale_codec::Error> {
		let product_id = String::decode(input)?;
		let execution_kind = ProductExecutionKind::decode(input)?;
		Self::new_with_execution(product_id, execution_kind)
			.map_err(|_| "ProductContext.product_id is not an accepted product identifier".into())
	}
}

/// Whether `identifier` is a product scope the core is allowed to derive for.
pub fn is_product_identifier(identifier: &str) -> bool {
	normalize_product_identifier(identifier).is_ok()
}

/// Top-level domains that dotNS deployments register product names under.
/// Each network declares its own, so the set spans every network a host can
/// be pointed at rather than just the production one.
pub const DOTNS_TLDS: &[&str] = &["dot", "paseo", "testnet"];

/// Whether `normalized` ends in one of [`DOTNS_TLDS`]. Expects an
/// already-lowercased host with no trailing root dot.
pub fn has_dotns_tld(normalized: &str) -> bool {
	normalized.rsplit_once('.').is_some_and(|(_, tld)| DOTNS_TLDS.contains(&tld))
}

/// Blessed product labels across every network in [`DOTNS_TLDS`].
///
/// These products bypass recorded permissions and prompt only for device access.
pub const REMOTE_PERMISSION_TRUSTED_LABELS: &[&str] =
	&["peopl", GAME_PRODUCT_LABEL, "stash", PERSON_CHAT_PRODUCT_LABEL];

/// The bare label of the game product, Jollity, on every network.
const GAME_PRODUCT_LABEL: &str = "dim2";

/// The bare label of the person-to-person chat product on every network.
const PERSON_CHAT_PRODUCT_LABEL: &str = "chat";

/// Hosts available to every product unless a stored permission decision blocks them.
pub const BLESSED_REMOTE_DOMAINS: &[&str] = &["fonts.googleapis.com", "fonts.gstatic.com"];

/// Whether `product_id` holds every [`RemotePermission`] without prompting.
///
/// Expects the [`normalize_product_identifier`] form. Matches the whole label
/// and nothing else: `peopl.dot` and `peopl.paseo` are trusted, while
/// `app.peopl.dot` and any `localhost` identifier are separate products and are
/// not. The label is only read out of an id that [`has_dotns_tld`] accepts, so a
/// widened product-id policy cannot promote an arbitrary single-label host.
pub fn has_trusted_remote_permissions(product_id: &str) -> bool {
	dotns_product_label(product_id)
		.is_some_and(|label| REMOTE_PERMISSION_TRUSTED_LABELS.contains(&label))
}

/// Everything before the TLD of a dotNS `product_id`: `dim2` for `dim2.dot`,
/// `app.dim2` for `app.dim2.dot`. `None` when the id does not end in one of
/// [`DOTNS_TLDS`], so a `localhost` id never yields a label.
///
/// Expects the [`normalize_product_identifier`] form.
fn dotns_product_label(product_id: &str) -> Option<&str> {
	if !has_dotns_tld(product_id) {
		return None;
	}
	product_id.rsplit_once('.').map(|(label, _tld)| label)
}

/// Whether `product_id` in any accepted spelling holds every
/// [`RemotePermission`] without prompting.
///
/// [`has_trusted_remote_permissions`] reads the normalized form, which is what
/// the core always holds. A host holds whatever spelling it received, so this
/// normalizes first and answers `false` for an id that does not normalize at
/// all: an unrecognised spelling is never read as trusted.
///
/// Recorded permission decisions do not affect this policy.
pub fn normalizes_to_trusted_remote_permissions(product_id: &str) -> bool {
	normalize_product_identifier(product_id)
		.is_ok_and(|normalized| has_trusted_remote_permissions(&normalized))
}

/// The label a development product identifier is served under.
const LOCALHOST_PRODUCT_LABEL: &str = "localhost";

/// Whether `product_id` is a development localhost product identifier.
///
/// Exactly `localhost`, or `localhost:` followed by a port and nothing else.
/// Hosts grant these a development wildcard, so the match is the whole
/// identifier: a prefix test would also admit `localhost:8080.evil`, and a
/// label test would admit a published `localhost.<tld>`, which
/// [`normalize_product_identifier`] rejects for the same reason.
pub fn is_localhost_product_identifier(product_id: &str) -> bool {
	if product_id == LOCALHOST_PRODUCT_LABEL {
		return true;
	}
	product_id
		.strip_prefix("localhost:")
		.is_some_and(|port| !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()))
}

/// Largest accepted product identifier, in bytes.
///
/// Bounds the size of one identifier, not how many exist: a manifest miss
/// caches its answer keyed by the target and nothing evicts those entries.
/// Matches the cap on product-supplied chat identifiers.
pub const PRODUCT_ID_MAX_BYTES: usize = 256;

/// Normalize product identifiers before derivation and policy checks.
pub fn normalize_product_identifier(
	product_id: &str,
) -> Result<String, RuntimeConfigValidationError> {
	let trimmed = product_id.trim();
	require_non_empty("product_id", trimmed)?;
	let normalized = trimmed.nfc().collect::<String>().to_lowercase();
	// After normalizing, since NFC can change the length. Reported by length
	// rather than by value: an id that trips this can be arbitrarily large.
	if normalized.len() > PRODUCT_ID_MAX_BYTES {
		return Err(RuntimeConfigValidationError::ProductIdTooLong {
			limit: PRODUCT_ID_MAX_BYTES,
			actual: normalized.len(),
		});
	}
	if !has_well_formed_labels(&normalized) {
		return Err(RuntimeConfigValidationError::InvalidProductId {
			product_id: product_id.to_string(),
		});
	}
	if is_localhost_product_identifier(&normalized) {
		return Ok(normalized);
	}
	// `localhost` is reserved as a label, not only as a bare identifier. A
	// product published as `localhost.<tld>` would otherwise be one string
	// comparison away from whatever a host grants the development wildcard.
	let reserves_localhost = normalized
		.rsplit_once('.')
		.is_some_and(|(label, _tld)| label == LOCALHOST_PRODUCT_LABEL);
	if has_dotns_tld(&normalized) && !reserves_localhost {
		Ok(normalized)
	} else {
		Err(RuntimeConfigValidationError::InvalidProductId { product_id: product_id.to_string() })
	}
}

/// Whether every dot-separated label of a normalized id is well formed.
///
/// Only the suffix after the last `.` was ever inspected, so the rest of the id
/// could be anything the transport carried. Three consequences, all reachable
/// because a cross-product call takes this string from the wire where it is
/// self-asserted:
///
/// - An empty label. `dim2..dot` and `.dot` both reduce to `""`, so distinct ids collapse onto one
///   grant key and one manifest cache entry.
/// - Control and format characters. A NUL or a right-to-left override survives into
///   [`AccountAccessReview`], which renders the id verbatim in the only consent prompt this design
///   has, and into every log line carrying it.
/// - Whitespace and path separators, which let one product's id render like another's anywhere the
///   comparison is not byte-exact.
///
/// Deliberately not an ASCII allowlist: dotNS names are internationalized, so
/// `tést.dot` is a real id. What is rejected is the class of characters that
/// carries no name and only confuses a reader or a key.
fn has_well_formed_labels(normalized: &str) -> bool {
	/// Zero-width and bidirectional formatting characters. Invisible in every
	/// rendering, so they make two different ids look identical to a user.
	fn is_invisible_format(c: char) -> bool {
		matches!(c,
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{2069}'
            | '\u{feff}')
	}
	let label_ok = |label: &str| {
		!label.is_empty() &&
			!label.chars().any(|c| {
				c.is_control() ||
					c.is_whitespace() ||
					is_invisible_format(c) ||
					matches!(c, '/' | '\\')
			})
	};
	if normalized == "localhost" {
		return true;
	}
	if let Some(port) = normalized.strip_prefix("localhost:") {
		return !port.is_empty() && port.chars().all(|c| c.is_ascii_digit());
	}
	normalized.split('.').all(label_ok)
}

/// Largest accepted length for a product-supplied chat identifier or display
/// name, in bytes.
pub const CHAT_FIELD_MAX_BYTES: usize = 256;

/// Largest accepted length for a product-supplied chat icon, in bytes. Wide
/// enough for a `data:` thumbnail, far below the transport frame cap.
pub const CHAT_ICON_MAX_BYTES: usize = 64 * 1024;

/// Inline image media types a chat icon may carry. SVG is excluded: it can
/// carry script.
const ALLOWED_ICON_DATA_TYPES: [&str; 5] =
	["image/png", "image/jpeg", "image/gif", "image/webp", "image/avif"];

/// Normalize a product-supplied chat room or bot identifier.
///
/// Screened harder than a display name, mirroring
/// [`normalize_product_identifier`]: an identifier is matched, not read, so it
/// also rejects the invisible characters a name legitimately needs — joiners,
/// variation selectors, soft hyphens and non-ASCII spaces — which would
/// otherwise let two distinct ids render identically.
pub fn normalize_chat_identifier(field: &'static str, id: &str) -> Result<String, ChatFieldError> {
	let normalized = normalize_chat_text(field, id)?;
	if normalized.is_empty() {
		return Err(ChatFieldError::Empty { field });
	}
	if normalized.chars().any(is_identifier_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}
	Ok(normalized)
}

/// Validate a product-supplied chat display name.
pub fn validate_chat_name(field: &'static str, name: &str) -> Result<String, ChatFieldError> {
	normalize_chat_text(field, name)
}

/// Trim, NFC-normalize and screen one product-supplied chat string.
///
/// The byte budget applies to the normalized value, which is what a host
/// receives: NFC can expand the input.
fn normalize_chat_text(field: &'static str, value: &str) -> Result<String, ChatFieldError> {
	let normalized = value.trim().nfc().collect::<String>();
	if normalized.len() > CHAT_FIELD_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_FIELD_MAX_BYTES });
	}
	if normalized.chars().any(is_display_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}
	Ok(normalized)
}

/// Largest accepted length for a product-supplied message body, in bytes.
pub const CHAT_BODY_MAX_BYTES: usize = 16 * 1024;

/// Largest accepted length for a product-supplied message URL, in bytes.
pub const CHAT_URL_MAX_BYTES: usize = 2048;

/// Largest accepted number of action buttons on one message.
pub const CHAT_ACTIONS_MAX: usize = 32;

/// Largest accepted number of media items on one message.
pub const CHAT_MEDIA_MAX: usize = 32;

/// Largest accepted size for a custom message payload, in bytes.
pub const CHAT_CUSTOM_PAYLOAD_MAX_BYTES: usize = 256 * 1024;

/// Validate and normalize product-supplied message content.
///
/// Every field here is authored by the product and rendered by the host. Names
/// and identifiers get the treatment [`validate_chat_name`] and
/// [`validate_chat_icon`] give a room: bounded, NFC-normalized, and screened
/// for characters that let two values render alike. A body is bounded and
/// screened but otherwise untouched, because it is content rather than a
/// label. Anything a host may fetch or open is restricted to schemes an
/// allowlist recognizes.
///
/// The counts matter as much as the byte budgets: the transport frame cap
/// alone allows millions of action buttons in one call.
pub fn validate_chat_message_content(
	content: ChatMessageContent,
) -> Result<ChatMessageContent, ChatFieldError> {
	use ChatMessageContent as Content;
	Ok(match content {
		Content::Text { text } => Content::Text { text: validate_chat_body("text", &text)? },
		Content::RichText(rich) => Content::RichText(ChatRichText {
			text: rich.text.map(|t| validate_chat_body("text", &t)).transpose()?,
			media: validate_chat_media("media", rich.media)?,
		}),
		Content::Actions(actions) => Content::Actions(validate_chat_action_set(actions)?),
		Content::File(file) => Content::File(ChatFile {
			url: validate_chat_url("url", &file.url)?,
			file_name: validate_chat_file_name("fileName", &file.file_name)?,
			mime_type: validate_chat_name("mimeType", &file.mime_type)?,
			size_bytes: file.size_bytes,
			text: file.text.map(|t| validate_chat_body("text", &t)).transpose()?,
		}),
		Content::Reaction(reaction) => Content::Reaction(validate_chat_reaction(reaction)?),
		Content::ReactionRemoved(reaction) => {
			Content::ReactionRemoved(validate_chat_reaction(reaction)?)
		},
		Content::Custom(custom) => {
			if custom.payload.len() > CHAT_CUSTOM_PAYLOAD_MAX_BYTES {
				return Err(ChatFieldError::TooLong {
					field: "payload",
					limit: CHAT_CUSTOM_PAYLOAD_MAX_BYTES,
				});
			}
			Content::Custom(ChatCustomMessage {
				message_type: normalize_chat_identifier("messageType", &custom.message_type)?,
				payload: custom.payload,
			})
		},
	})
}

/// Validate a product-supplied file name.
///
/// Screened as a display name, because a bidi override reverses the extension a
/// host shows on a download affordance, and additionally as a path component:
/// a host that joins this onto a cache directory must not be handed separators
/// or a parent reference.
fn validate_chat_file_name(field: &'static str, name: &str) -> Result<String, ChatFieldError> {
	let validated = validate_chat_name(field, name)?;
	if validated.is_empty() {
		return Err(ChatFieldError::Empty { field });
	}
	if validated.contains(['/', '\\', ':']) ||
		validated == ".." ||
		validated == "." ||
		validated.starts_with("..")
	{
		return Err(ChatFieldError::PathComponent { field });
	}
	Ok(validated)
}

/// Validate the text and buttons of an `Actions` message.
fn validate_chat_action_set(actions: ChatActions) -> Result<ChatActions, ChatFieldError> {
	if actions.actions.len() > CHAT_ACTIONS_MAX {
		return Err(ChatFieldError::TooMany { field: "actions", limit: CHAT_ACTIONS_MAX });
	}
	Ok(ChatActions {
		text: actions.text.map(|t| validate_chat_body("text", &t)).transpose()?,
		actions: validate_chat_actions(actions.actions)?,
		layout: actions.layout,
	})
}

/// Validate one message's action buttons.
///
/// Ids are normalized, which can map two spellings onto one key, so the
/// normalized set is checked for collisions: a product shipping both `approve`
/// and ` approve ` would otherwise get one button, and a trigger naming that
/// key could not say which was pressed.
fn validate_chat_actions(actions: Vec<ChatAction>) -> Result<Vec<ChatAction>, ChatFieldError> {
	let mut seen = BTreeSet::new();
	actions
		.into_iter()
		.map(|action| {
			let action_id = normalize_chat_identifier("actionId", &action.action_id)?;
			if !seen.insert(action_id.clone()) {
				return Err(ChatFieldError::Duplicate { field: "actionId" });
			}
			Ok(ChatAction { action_id, title: validate_chat_name("title", &action.title)? })
		})
		.collect()
}

/// Validate a reaction: the message it names is an identifier, matched rather
/// than read, so it is screened like one.
fn validate_chat_reaction(reaction: ChatReaction) -> Result<ChatReaction, ChatFieldError> {
	Ok(ChatReaction {
		message_id: normalize_chat_identifier("messageId", &reaction.message_id)?,
		emoji: validate_chat_emoji("emoji", &reaction.emoji)?,
	})
}

fn validate_chat_media(
	field: &'static str,
	media: Vec<ChatMedia>,
) -> Result<Vec<ChatMedia>, ChatFieldError> {
	if media.len() > CHAT_MEDIA_MAX {
		return Err(ChatFieldError::TooMany { field, limit: CHAT_MEDIA_MAX });
	}
	media
		.into_iter()
		.map(|item| Ok(ChatMedia { url: validate_chat_url("url", &item.url)? }))
		.collect()
}

/// Bound and screen a product-authored message body.
///
/// A body is opaque content rather than a label, so unlike a name it is
/// neither trimmed nor NFC-normalized: a product that hashes, signs or
/// echo-compares what it sent reads back the same bytes, and leading
/// indentation in a code block survives.
fn validate_chat_body(field: &'static str, value: &str) -> Result<String, ChatFieldError> {
	if value.len() > CHAT_BODY_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_BODY_MAX_BYTES });
	}
	if value.chars().any(is_body_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}
	Ok(value.to_string())
}

/// Bound and screen a product-supplied reaction emoji.
fn validate_chat_emoji(field: &'static str, value: &str) -> Result<String, ChatFieldError> {
	let normalized = value.trim().nfc().collect::<String>();
	if normalized.len() > CHAT_FIELD_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_FIELD_MAX_BYTES });
	}
	if normalized.chars().any(is_emoji_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}
	Ok(normalized)
}

/// Resolve a product-supplied `https` URL to what a host will actually be
/// handed, or reject it.
///
/// Both chat URL fields route through here so the budget is always measured
/// against the resolved string. The parser percent-encodes, and a non-ASCII
/// path triples in the process, so a value that arrives inside its cap can
/// leave well past it.
///
/// Credentials are refused rather than carried: `Url::to_string` keeps
/// `user:pass@`, so a host handed one would fetch with them and log them.
///
/// Which hosts are reachable is deliberately not decided here. See
/// [`validate_chat_url`].
fn resolve_chat_https(
	field: &'static str,
	trimmed: &str,
	limit: usize,
) -> Result<String, ChatFieldError> {
	let parsed = Url::parse(trimmed)
		.ok()
		.filter(|parsed| parsed.scheme() == "https")
		.ok_or(ChatFieldError::RejectedScheme { field })?;
	if !parsed.username().is_empty() || parsed.password().is_some() {
		return Err(ChatFieldError::Credentials { field });
	}
	// The resolved target, not the arriving string: a host renders what the
	// core validated, and the budget applies to what the host receives.
	let resolved = parsed.to_string();
	if resolved.len() > limit {
		return Err(ChatFieldError::TooLong { field, limit });
	}
	Ok(resolved)
}

/// Validate a URL a host may fetch or open.
///
/// The same allowlist [`validate_chat_icon`] applies, for the same reason: a
/// URL parser reaches a scheme through whitespace, tabs and NUL that a prefix
/// comparison does not, so `javascript:` and `file:` must be excluded by what
/// is permitted rather than by what is named.
fn validate_chat_url(field: &'static str, url: &str) -> Result<String, ChatFieldError> {
	let trimmed = url.trim();
	if trimmed.is_empty() {
		return Err(ChatFieldError::Empty { field });
	}
	// Screened before parsing: the parser drops tabs and newlines, so a string
	// it accepts is not the string a host would render.
	if trimmed.chars().any(is_display_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}
	match icon_scheme(trimmed).as_deref() {
		Some("https") => resolve_chat_https(field, trimmed, CHAT_URL_MAX_BYTES),
		// An inline image is measured against the icon budget; the link cap
		// would leave `data:` accepted but too small to carry an image.
		Some("data") if is_allowed_icon_data_url(trimmed) => {
			if trimmed.len() > CHAT_ICON_MAX_BYTES {
				return Err(ChatFieldError::TooLong { field, limit: CHAT_ICON_MAX_BYTES });
			}
			Ok(trimmed.to_string())
		},
		_ => Err(ChatFieldError::RejectedScheme { field }),
	}
}

/// Validate a product-supplied chat icon: absent, an `https` URL, or an inline
/// PNG, JPEG, GIF, WebP, or AVIF image.
///
/// An allowlist rather than a denylist, because a URL parser reaches a scheme
/// through whitespace, tabs and NUL that a prefix comparison does not.
pub fn validate_chat_icon(field: &'static str, icon: &str) -> Result<String, ChatFieldError> {
	let trimmed = icon.trim();
	if trimmed.is_empty() {
		return Ok(String::new());
	}
	if trimmed.len() > CHAT_ICON_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_ICON_MAX_BYTES });
	}
	if trimmed.chars().any(is_display_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}

	match icon_scheme(trimmed).as_deref() {
		Some("https") => resolve_chat_https(field, trimmed, CHAT_ICON_MAX_BYTES),
		Some("data") if is_allowed_icon_data_url(trimmed) => Ok(trimmed.to_string()),
		_ => Err(ChatFieldError::RejectedScheme { field }),
	}
}

/// Largest accepted length for a room or command description, in bytes.
pub const CHAT_DESCRIPTION_MAX_BYTES: usize = 1024;

/// Largest accepted size for an inline media item, in bytes.
pub const CHAT_INLINE_MEDIA_MAX_BYTES: usize = 1024 * 1024;

/// Length of a preimage key, in bytes: a BLAKE2b-256 hash.
pub const CHAT_PREIMAGE_KEY_BYTES: usize = 32;

/// Largest accepted number of reaction summaries on one message, and of
/// emoji in the reaction picker of a room.
pub const CHAT_REACTIONS_MAX: usize = 64;

/// Largest accepted number of product entries in one menu.
pub const CHAT_MENU_ACTIONS_MAX: usize = 16;

/// Largest page of messages a host may ask for.
pub const CHAT_MESSAGES_PAGE_MAX: u32 = 100;

/// Page of messages a host asks for when it has no reason to ask another.
pub const CHAT_MESSAGES_PAGE_DEFAULT: u32 = 40;

/// Largest page of participants a host may ask for.
pub const CHAT_PARTICIPANTS_PAGE_MAX: u32 = 200;

/// Page of participants a host asks for when it has no reason to ask another.
pub const CHAT_PARTICIPANTS_PAGE_DEFAULT: u32 = 100;

/// Largest accepted number of participant ids in one `ByIds` question.
pub const CHAT_PARTICIPANT_IDS_MAX: usize = 100;

/// Largest accepted length for a page cursor or a deeplink, in bytes.
pub const CHAT_CURSOR_MAX_BYTES: usize = 1024;

/// Largest accepted number of `update_feed` calls for one room in one second.
pub const CHAT_FEED_UPDATES_PER_SECOND: u32 = 20;

/// Largest accepted number of `set_room_preview` or `set_room_activity` calls
/// for one room in one second.
pub const CHAT_PREVIEW_UPDATES_PER_SECOND: u32 = 5;

/// Largest attachment a host stores as a preimage, in bytes.
pub const CHAT_PREIMAGE_ATTACHMENT_MAX_BYTES: usize = 8 * 1024 * 1024;

/// Largest attachment a host seals, in bytes (`sealed_attachments`).
pub const CHAT_SEALED_ATTACHMENT_MAX_BYTES: usize = 32 * 1024 * 1024;

/// Largest accepted number of parts of one sealed resource.
pub const CHAT_SEALED_PARTS_MAX: usize = 64;

/// The largest file a [`ChatResource::Hop`] names: the largest the native chat sends.
pub const CHAT_HOP_FILE_MAX_BYTES: u64 = 128 * 1024 * 1024;

/// The longest node URL of a [`ChatResource::Hop`].
pub const CHAT_HOP_NODE_MAX_CHARS: usize = 256;

/// The longest media signal of a `ChatRoomEvent::CallSignal`.
pub const CHAT_CALL_SIGNAL_MAX_BYTES: usize = 64 * 1024;

/// Length of the authentication tag that the sealing adds, in bytes.
pub const CHAT_SEALED_TAG_BYTES: usize = 16;

/// Largest accepted number of feature names in `capabilities`.
pub const CHAT_FEATURES_MAX: usize = 64;

/// Largest page of search matches a host may ask for.
pub const CHAT_SEARCH_PAGE_MAX: u32 = 50;

/// Page of search matches a host asks for when it has no reason to ask
/// another.
pub const CHAT_SEARCH_PAGE_DEFAULT: u32 = 30;

/// Largest accepted number of personas of one product.
pub const CHAT_PERSONAS_MAX: usize = 8;

/// Largest accepted number of `set_room_call` calls for one room in one
/// second.
pub const CHAT_CALL_UPDATES_PER_SECOND: u32 = 2;

/// Largest number of room events the core keeps for a product that has no
/// subscriber. The oldest event goes first.
pub const CHAT_ROOM_EVENTS_QUEUED_MAX: usize = 64;

/// The prefix of the participant id of a persona: `bot:<bot_id>`.
pub const CHAT_PERSONA_PREFIX: &str = "bot:";

/// The Chat feature names of the Chat Extension v2 additions RFC, in the
/// order the core lists them. A host lists a name only when it serves that
/// feature from end to end.
pub const CHAT_FEATURE_NAMES: [&str; 20] = [
	"product_source_rooms",
	"reaction_details",
	"participants",
	"message_menus",
	"custom_bodies",
	"custom_footer",
	"inline_attachments",
	"payment_button",
	"room_links",
	"preimage_attachments",
	"sealed_attachments",
	"hop_attachments",
	"search",
	"bot_personas",
	"mute_archive",
	"read_aloud",
	"wake",
	"call_buttons",
	"room_calls",
	"room_link_payloads",
];

/// The bot id of a persona participant id `bot:<bot_id>`, or `None` for
/// any other participant id.
pub fn chat_persona_bot_id(participant_id: &str) -> Option<&str> {
	participant_id.strip_prefix(CHAT_PERSONA_PREFIX)
}

/// The feature names a host lists that the RFC knows, each once, in the order
/// of [`CHAT_FEATURE_NAMES`]. The core drops a name it does not know.
pub fn known_chat_feature_names(names: &[String]) -> Vec<&'static str> {
	CHAT_FEATURE_NAMES
		.into_iter()
		.filter(|known| names.iter().any(|name| name.trim() == *known))
		.collect()
}

/// Validate the call of a room.
pub fn validate_chat_room_call(call: ChatRoomCall) -> Result<ChatRoomCall, ChatFieldError> {
	let call_id = |id: &str| normalize_chat_identifier("callId", id);
	Ok(match call {
		ChatRoomCall::Ringing { call_id: id, video, caller } => ChatRoomCall::Ringing {
			call_id: call_id(&id)?,
			video,
			caller: caller
				.map(|caller| normalize_chat_identifier("caller", &caller))
				.transpose()?,
		},
		ChatRoomCall::Active { call_id: id, video, started_at, media_session } => {
			ChatRoomCall::Active { call_id: call_id(&id)?, video, started_at, media_session }
		},
	})
}

/// Validate an event a host reports about a room.
pub fn validate_chat_room_event(event: ChatRoomEvent) -> Result<ChatRoomEvent, ChatFieldError> {
	let call_id = |id: &str| normalize_chat_identifier("callId", id);
	Ok(match event {
		ChatRoomEvent::Woken { watch_id } => {
			ChatRoomEvent::Woken { watch_id: normalize_chat_identifier("watchId", &watch_id)? }
		},
		ChatRoomEvent::CallAnswered { call_id: id, accept } => {
			ChatRoomEvent::CallAnswered { call_id: call_id(&id)?, accept }
		},
		ChatRoomEvent::CallOpened { call_id: id } => {
			ChatRoomEvent::CallOpened { call_id: call_id(&id)? }
		},
		ChatRoomEvent::CallEnded { call_id: id } => {
			ChatRoomEvent::CallEnded { call_id: call_id(&id)? }
		},
		ChatRoomEvent::CallSignal { call_id: id, signal } => {
			if signal.is_empty() {
				return Err(ChatFieldError::Empty { field: "signal" });
			}
			if signal.len() > CHAT_CALL_SIGNAL_MAX_BYTES {
				return Err(ChatFieldError::TooLong {
					field: "signal",
					limit: CHAT_CALL_SIGNAL_MAX_BYTES,
				});
			}
			ChatRoomEvent::CallSignal { call_id: call_id(&id)?, signal }
		},
		event @ (ChatRoomEvent::UserStateChanged { .. } | ChatRoomEvent::CallRequested { .. }) => {
			event
		},
	})
}

/// Validate a search a host asks a product.
pub fn validate_chat_search_query(
	request: ProductChatSearchQueryRequest,
) -> Result<ProductChatSearchQueryRequest, ChatFieldError> {
	let text = normalize_chat_text("text", &request.text)?;
	if text.is_empty() {
		return Err(ChatFieldError::Empty { field: "text" });
	}
	Ok(ProductChatSearchQueryRequest {
		room_id: normalize_chat_identifier("roomId", &request.room_id)?,
		text,
		cursor: request
			.cursor
			.map(|cursor| validate_chat_cursor("cursor", cursor))
			.transpose()?,
		page_size: check_chat_page_size(request.page_size, CHAT_SEARCH_PAGE_MAX)?,
	})
}

/// Validate the matches a product answers for a search of `page_size`.
pub fn validate_chat_search_answer(
	answer: ProductChatSearchQueryItem,
	page_size: u32,
) -> Result<ProductChatSearchQueryItem, ChatFieldError> {
	check_chat_count("hits", &answer.hits, page_size as usize)?;
	Ok(ProductChatSearchQueryItem {
		hits: answer.hits.into_iter().map(validate_chat_message).collect::<Result<_, _>>()?,
		next: answer.next.map(|next| validate_chat_cursor("next", next)).transpose()?,
	})
}

/// Largest accepted payload of a room link, in bytes of UTF-8.
pub const CHAT_LINK_PAYLOAD_MAX_BYTES: usize = 8 * 1024;

/// Validate the payload of a room link a host gives a product. The core
/// checks only its size: the payload is the product's own data.
pub fn validate_chat_open_link(
	request: ProductChatOpenLinkRequest,
) -> Result<ProductChatOpenLinkRequest, ChatFieldError> {
	if request.payload.is_empty() {
		return Err(ChatFieldError::Empty { field: "payload" });
	}
	if request.payload.len() > CHAT_LINK_PAYLOAD_MAX_BYTES {
		return Err(ChatFieldError::TooLong {
			field: "payload",
			limit: CHAT_LINK_PAYLOAD_MAX_BYTES,
		});
	}
	Ok(ProductChatOpenLinkRequest {
		room_id: normalize_chat_identifier("roomId", &request.room_id)?,
		payload: request.payload,
	})
}

/// Validate the answer of a product to an opened room link.
pub fn validate_chat_open_link_answer(
	answer: ProductChatOpenLinkItem,
) -> Result<ProductChatOpenLinkItem, ChatFieldError> {
	Ok(ProductChatOpenLinkItem {
		room_id: answer
			.room_id
			.map(|room_id| normalize_chat_identifier("roomId", &room_id))
			.transpose()?,
	})
}

/// Validate a resource the host draws as an avatar, an icon or a badge: a
/// raster image of at most [`CHAT_ICON_MAX_BYTES`], or a preimage key.
pub fn validate_chat_image_resource(
	field: &'static str,
	resource: ChatResource,
) -> Result<ChatResource, ChatFieldError> {
	match resource {
		ChatResource::Inline { mime_type, bytes } => {
			let mime_type = validate_chat_mime_type(field, &mime_type)?;
			if !ALLOWED_ICON_DATA_TYPES.contains(&mime_type.as_str()) {
				return Err(ChatFieldError::RejectedScheme { field });
			}
			Ok(ChatResource::Inline {
				mime_type,
				bytes: validate_chat_inline_bytes(field, bytes, CHAT_ICON_MAX_BYTES)?,
			})
		},
		ChatResource::Preimage { key } => validate_chat_preimage_key(field, key),
		ChatResource::Sealed { .. } | ChatResource::Hop { .. } => {
			Err(ChatFieldError::RejectedScheme { field })
		},
	}
}

/// Validate the resource of a media item: inline bytes of at most
/// [`CHAT_INLINE_MEDIA_MAX_BYTES`], a preimage key, sealed parts, or a HOP file.
fn validate_chat_media_resource(
	field: &'static str,
	resource: ChatResource,
) -> Result<ChatResource, ChatFieldError> {
	match resource {
		ChatResource::Inline { mime_type, bytes } => Ok(ChatResource::Inline {
			mime_type: validate_chat_mime_type(field, &mime_type)?,
			bytes: validate_chat_inline_bytes(field, bytes, CHAT_INLINE_MEDIA_MAX_BYTES)?,
		}),
		ChatResource::Preimage { key } => validate_chat_preimage_key(field, key),
		sealed @ ChatResource::Sealed { .. } => validate_chat_sealed(field, sealed),
		hop @ ChatResource::Hop { .. } => validate_chat_hop(field, hop),
	}
}

/// Check the shape of a HOP file: a 32-byte identifier and ticket, a `wss://`
/// node of at most [`CHAT_HOP_NODE_MAX_CHARS`], and a size of 1 to
/// [`CHAT_HOP_FILE_MAX_BYTES`].
fn validate_chat_hop(
	field: &'static str,
	resource: ChatResource,
) -> Result<ChatResource, ChatFieldError> {
	let ChatResource::Hop { identifier, ticket, node, size } = &resource else {
		return Ok(resource);
	};
	if identifier.len() != 32 || ticket.len() != 32 {
		return Err(ChatFieldError::WrongLength { field, length: 32 });
	}
	if !node.starts_with("wss://") {
		return Err(ChatFieldError::RejectedScheme { field });
	}
	if node.chars().count() > CHAT_HOP_NODE_MAX_CHARS {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_HOP_NODE_MAX_CHARS });
	}
	if *size == 0 || *size > CHAT_HOP_FILE_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_HOP_FILE_MAX_BYTES as usize });
	}
	Ok(resource)
}

/// Check the shape of a sealed resource: 1 to [`CHAT_SEALED_PARTS_MAX`] part
/// keys, a 32-byte key, a 24-byte nonce, a SHA-512 digest, and a ciphertext
/// size of at most [`CHAT_SEALED_ATTACHMENT_MAX_BYTES`] and its tag.
fn validate_chat_sealed(
	field: &'static str,
	resource: ChatResource,
) -> Result<ChatResource, ChatFieldError> {
	let ChatResource::Sealed { parts, key, nonce, size, digest } = &resource else {
		return Ok(resource);
	};
	if parts.is_empty() {
		return Err(ChatFieldError::Empty { field });
	}
	if parts.len() > CHAT_SEALED_PARTS_MAX {
		return Err(ChatFieldError::TooMany { field, limit: CHAT_SEALED_PARTS_MAX });
	}
	let wrong = |length| ChatFieldError::WrongLength { field, length };
	if parts.iter().any(|part| part.len() != CHAT_PREIMAGE_KEY_BYTES) {
		return Err(wrong(CHAT_PREIMAGE_KEY_BYTES));
	}
	if key.len() != 32 {
		return Err(wrong(32));
	}
	if nonce.len() != 24 {
		return Err(wrong(24));
	}
	if digest.len() != 64 {
		return Err(wrong(64));
	}
	let limit = CHAT_SEALED_ATTACHMENT_MAX_BYTES + CHAT_SEALED_TAG_BYTES;
	if *size <= CHAT_SEALED_TAG_BYTES as u64 || *size > limit as u64 {
		return Err(ChatFieldError::TooLong { field, limit });
	}
	Ok(resource)
}

/// A media type is matched against allowlists, so it is lower-cased and
/// screened like a name.
fn validate_chat_mime_type(field: &'static str, mime_type: &str) -> Result<String, ChatFieldError> {
	let mime_type = validate_chat_name(field, mime_type)?.to_ascii_lowercase();
	if mime_type.is_empty() {
		return Err(ChatFieldError::Empty { field });
	}
	Ok(mime_type)
}

fn validate_chat_inline_bytes(
	field: &'static str,
	bytes: Vec<u8>,
	limit: usize,
) -> Result<Vec<u8>, ChatFieldError> {
	if bytes.is_empty() {
		return Err(ChatFieldError::Empty { field });
	}
	if bytes.len() > limit {
		return Err(ChatFieldError::TooLong { field, limit });
	}
	Ok(bytes)
}

fn validate_chat_preimage_key(
	field: &'static str,
	key: Vec<u8>,
) -> Result<ChatResource, ChatFieldError> {
	if key.len() != CHAT_PREIMAGE_KEY_BYTES {
		return Err(ChatFieldError::WrongLength { field, length: CHAT_PREIMAGE_KEY_BYTES });
	}
	Ok(ChatResource::Preimage { key })
}

/// Bound and screen a description. It is content that can span lines, so it
/// keeps line breaks and is not normalized.
fn validate_chat_description(field: &'static str, value: &str) -> Result<String, ChatFieldError> {
	if value.len() > CHAT_DESCRIPTION_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_DESCRIPTION_MAX_BYTES });
	}
	validate_chat_body(field, value)
}

/// Bound and screen an opaque page cursor.
fn validate_chat_cursor(field: &'static str, cursor: String) -> Result<String, ChatFieldError> {
	if cursor.len() > CHAT_CURSOR_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_CURSOR_MAX_BYTES });
	}
	if cursor.chars().any(is_display_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}
	Ok(cursor)
}

/// Reject a list longer than `limit`.
fn check_chat_count<T>(
	field: &'static str,
	items: &[T],
	limit: usize,
) -> Result<(), ChatFieldError> {
	if items.len() > limit {
		return Err(ChatFieldError::TooMany { field, limit });
	}
	Ok(())
}

/// Normalize identifiers that must be unique in one list.
fn unique_chat_identifier(
	field: &'static str,
	id: &str,
	seen: &mut BTreeSet<String>,
) -> Result<String, ChatFieldError> {
	let id = normalize_chat_identifier(field, id)?;
	if !seen.insert(id.clone()) {
		return Err(ChatFieldError::Duplicate { field });
	}
	Ok(id)
}

/// Validate the name, avatar and description of a room.
pub fn validate_chat_room_metadata(
	metadata: ChatRoomMetadata,
) -> Result<ChatRoomMetadata, ChatFieldError> {
	Ok(ChatRoomMetadata {
		name: validate_chat_name("name", &metadata.name)?,
		avatar: metadata
			.avatar
			.map(|avatar| validate_chat_image_resource("avatar", avatar))
			.transpose()?,
		description: metadata
			.description
			.map(|description| validate_chat_description("description", &description))
			.transpose()?,
	})
}

/// Validate the configuration of a room.
pub fn validate_chat_room_config(config: ChatRoomConfig) -> Result<ChatRoomConfig, ChatFieldError> {
	let allowed_reactions = match config.allowed_reactions {
		Some(emoji) => {
			check_chat_count("allowedReactions", &emoji, CHAT_REACTIONS_MAX)?;
			let mut seen = BTreeSet::new();
			let emoji = emoji
				.iter()
				.map(|emoji| {
					let emoji = validate_chat_emoji("allowedReactions", emoji)?;
					if emoji.is_empty() {
						return Err(ChatFieldError::Empty { field: "allowedReactions" });
					}
					if !seen.insert(emoji.clone()) {
						return Err(ChatFieldError::Duplicate { field: "allowedReactions" });
					}
					Ok(emoji)
				})
				.collect::<Result<_, _>>()?;
			Some(emoji)
		},
		None => None,
	};
	Ok(ChatRoomConfig {
		allowed_reactions,
		placeholder: config
			.placeholder
			.map(|placeholder| validate_chat_body("placeholder", &placeholder))
			.transpose()?,
		..config
	})
}

/// Validate how a room shows in the chat list.
pub fn validate_chat_room_preview(
	preview: ChatRoomPreview,
) -> Result<ChatRoomPreview, ChatFieldError> {
	Ok(ChatRoomPreview {
		order: preview.order,
		timestamp: preview.timestamp,
		badge: match preview.badge {
			Some(ChatRoomBadge::Image { resource }) => Some(ChatRoomBadge::Image {
				resource: validate_chat_image_resource("badge", resource)?,
			}),
			badge => badge,
		},
		text: preview
			.text
			.map(|text| {
				Ok::<_, ChatFieldError>(ChatRoomPreviewText {
					text: validate_chat_name("text", &text.text)?,
					ongoing: text.ongoing,
				})
			})
			.transpose()?,
	})
}

/// Validate what a message says.
///
/// A size rejection names `text` or `payload`, the fields a product shrinks
/// by sending less.
pub fn validate_chat_message_body(
	body: ChatMessageBody,
) -> Result<ChatMessageBody, ChatFieldError> {
	Ok(match body {
		ChatMessageBody::Text { text } => {
			ChatMessageBody::Text { text: validate_chat_body("text", &text)? }
		},
		ChatMessageBody::RichText { text, media } => {
			check_chat_count("media", &media, CHAT_MEDIA_MAX)?;
			ChatMessageBody::RichText {
				text: text.map(|text| validate_chat_body("text", &text)).transpose()?,
				media: media.into_iter().map(validate_chat_media_item).collect::<Result<_, _>>()?,
			}
		},
		ChatMessageBody::Actions(actions) => {
			ChatMessageBody::Actions(validate_chat_action_set(actions)?)
		},
		ChatMessageBody::Notice { text } => {
			ChatMessageBody::Notice { text: validate_chat_body("text", &text)? }
		},
		ChatMessageBody::Custom { message_type, payload, preview_text } => {
			if payload.len() > CHAT_CUSTOM_PAYLOAD_MAX_BYTES {
				return Err(ChatFieldError::TooLong {
					field: "payload",
					limit: CHAT_CUSTOM_PAYLOAD_MAX_BYTES,
				});
			}
			ChatMessageBody::Custom {
				message_type: normalize_chat_identifier("messageType", &message_type)?,
				payload,
				preview_text: preview_text
					.map(|preview| validate_chat_body("previewText", &preview))
					.transpose()?,
			}
		},
	})
}

fn validate_chat_media_item(item: ChatMediaItem) -> Result<ChatMediaItem, ChatFieldError> {
	Ok(match item {
		ChatMediaItem::Image { resource, meta } => ChatMediaItem::Image {
			resource: validate_chat_media_resource("media", resource)?,
			meta: validate_chat_media_meta(meta)?,
		},
		ChatMediaItem::Video { resource, meta } => ChatMediaItem::Video {
			resource: validate_chat_media_resource("media", resource)?,
			meta: validate_chat_media_meta(meta)?,
		},
		ChatMediaItem::File { resource, meta, file_name } => ChatMediaItem::File {
			resource: validate_chat_media_resource("media", resource)?,
			meta: validate_chat_media_meta(meta)?,
			file_name: validate_chat_file_name("fileName", &file_name)?,
		},
	})
}

fn validate_chat_media_meta(meta: ChatMediaMeta) -> Result<ChatMediaMeta, ChatFieldError> {
	Ok(ChatMediaMeta {
		mime_type: validate_chat_mime_type("mimeType", &meta.mime_type)?,
		placeholder: meta
			.placeholder
			.map(|placeholder| validate_chat_name("placeholder", &placeholder))
			.transpose()?,
		alt: meta.alt.map(|alt| validate_chat_name("alt", &alt)).transpose()?,
		..meta
	})
}

/// Validate one message, as a product writes it into a feed or answers it in
/// a page.
pub fn validate_chat_message(message: ChatMessage) -> Result<ChatMessage, ChatFieldError> {
	check_chat_count("reactions", &message.reactions, CHAT_REACTIONS_MAX)?;
	let mut seen = BTreeSet::new();
	let reactions = message
		.reactions
		.into_iter()
		.map(|reaction| {
			let emoji = validate_chat_emoji("emoji", &reaction.emoji)?;
			if !seen.insert(emoji.clone()) {
				return Err(ChatFieldError::Duplicate { field: "emoji" });
			}
			Ok(ChatReactionSummary { emoji, ..reaction })
		})
		.collect::<Result<_, _>>()?;
	Ok(ChatMessage {
		message_id: normalize_chat_identifier("messageId", &message.message_id)?,
		origin: ChatMessageOrigin {
			participant_id: normalize_chat_identifier(
				"participantId",
				&message.origin.participant_id,
			)?,
		},
		timestamp: message.timestamp,
		body: validate_chat_message_body(message.body)?,
		status: message.status,
		reply_to: message
			.reply_to
			.map(|reply_to| normalize_chat_identifier("replyTo", &reply_to))
			.transpose()?,
		edited_at: message.edited_at,
		reactions,
	})
}

/// Validate one change to the messages of a room.
pub fn validate_chat_feed_update(update: ChatFeedUpdate) -> Result<ChatFeedUpdate, ChatFieldError> {
	let message_id = |id: &str| normalize_chat_identifier("messageId", id);
	let participant_id = |id: &str| normalize_chat_identifier("participantId", id);
	Ok(match update {
		ChatFeedUpdate::New(message) => ChatFeedUpdate::New(validate_chat_message(message)?),
		ChatFeedUpdate::Edited { message_id: id, body, edited_at } => ChatFeedUpdate::Edited {
			message_id: message_id(&id)?,
			body: validate_chat_message_body(body)?,
			edited_at,
		},
		ChatFeedUpdate::Deleted { message_id: id } => {
			ChatFeedUpdate::Deleted { message_id: message_id(&id)? }
		},
		ChatFeedUpdate::ReactionPlaced { message_id: id, participant_id: by, emoji } => {
			ChatFeedUpdate::ReactionPlaced {
				message_id: message_id(&id)?,
				participant_id: participant_id(&by)?,
				emoji: validate_chat_emoji("emoji", &emoji)?,
			}
		},
		ChatFeedUpdate::ReactionRemoved { message_id: id, participant_id: by, emoji } => {
			ChatFeedUpdate::ReactionRemoved {
				message_id: message_id(&id)?,
				participant_id: participant_id(&by)?,
				emoji: validate_chat_emoji("emoji", &emoji)?,
			}
		},
		ChatFeedUpdate::StatusChanged { message_id: id, status } => {
			ChatFeedUpdate::StatusChanged { message_id: message_id(&id)?, status }
		},
	})
}

/// Validate what happens when the user selects a product action. A deeplink
/// must be a target `system.navigate_to` accepts, and the host receives the
/// form `navigate_to` would hand it.
fn validate_chat_action_handler(
	handler: ChatActionHandler,
) -> Result<ChatActionHandler, ChatFieldError> {
	use crate::host_logic::dotns::{NavigateDecision, parse_navigate};
	let ChatActionHandler::Deeplink { url } = handler else { return Ok(handler) };
	let field = "url";
	if url.len() > CHAT_CURSOR_MAX_BYTES {
		return Err(ChatFieldError::TooLong { field, limit: CHAT_CURSOR_MAX_BYTES });
	}
	if url.chars().any(is_display_unsafe) {
		return Err(ChatFieldError::UnsafeCharacter { field });
	}
	let url = match parse_navigate(&url) {
		NavigateDecision::Reject { .. } => return Err(ChatFieldError::RejectedScheme { field }),
		NavigateDecision::DotName { canonical_url, .. } |
		NavigateDecision::Localhost { canonical_url, .. } |
		NavigateDecision::Pocket { canonical_url, .. } |
		NavigateDecision::PocketCollection { canonical_url, .. } => canonical_url,
		NavigateDecision::External { url } => url,
	};
	Ok(ChatActionHandler::Deeplink { url })
}

/// Validate the product entries of one menu.
fn validate_chat_menu_actions(
	actions: Vec<ChatProductAction>,
) -> Result<Vec<ChatProductAction>, ChatFieldError> {
	check_chat_count("actions", &actions, CHAT_MENU_ACTIONS_MAX)?;
	let mut seen = BTreeSet::new();
	actions
		.into_iter()
		.map(|action| {
			Ok(ChatProductAction {
				action_id: unique_chat_identifier("actionId", &action.action_id, &mut seen)?,
				label: validate_chat_name("label", &action.label)?,
				icon: action
					.icon
					.map(|icon| validate_chat_image_resource("icon", icon))
					.transpose()?,
				destructive: action.destructive,
				handler: validate_chat_action_handler(action.handler)?,
			})
		})
		.collect()
}

/// Validate the menu a product gives for a message.
pub fn validate_chat_message_menu(
	menu: ChatMessageMenu,
) -> Result<ChatMessageMenu, ChatFieldError> {
	Ok(match menu {
		ChatMessageMenu::Custom { actions } => {
			ChatMessageMenu::Custom { actions: validate_chat_menu_actions(actions)? }
		},
		ChatMessageMenu::ExtendBuiltIn { actions, hidden } => {
			ChatMessageMenu::ExtendBuiltIn { actions: validate_chat_menu_actions(actions)?, hidden }
		},
	})
}

/// Validate what a room shows below its messages.
pub fn validate_chat_room_footer(footer: ChatRoomFooter) -> Result<ChatRoomFooter, ChatFieldError> {
	Ok(match footer {
		ChatRoomFooter::Input(input) => ChatRoomFooter::Input(ChatInputConfig {
			placeholder: input
				.placeholder
				.map(|placeholder| validate_chat_name("placeholder", &placeholder))
				.transpose()?,
			..input
		}),
		footer => footer,
	})
}

/// Validate a participant a product describes.
pub fn validate_chat_participant_entry(
	entry: ChatParticipantEntry,
) -> Result<ChatParticipantEntry, ChatFieldError> {
	Ok(ChatParticipantEntry {
		participant_id: normalize_chat_identifier("participantId", &entry.participant_id)?,
		participant: match entry.participant {
			ChatParticipant::Product { product_id } => ChatParticipant::Product {
				product_id: normalize_chat_identifier("productId", &product_id)?,
			},
			ChatParticipant::Custom { nickname, avatar } => ChatParticipant::Custom {
				nickname: validate_chat_name("nickname", &nickname)?,
				avatar: avatar
					.map(|avatar| validate_chat_image_resource("avatar", avatar))
					.transpose()?,
			},
			participant => participant,
		},
	})
}

/// Validate a page of messages a host asks for.
pub fn validate_chat_messages_query(
	request: ProductChatMessagesQueryRequest,
) -> Result<ProductChatMessagesQueryRequest, ChatFieldError> {
	let cursor = |cursor: Option<String>| {
		cursor.map(|cursor| validate_chat_cursor("cursor", cursor)).transpose()
	};
	Ok(ProductChatMessagesQueryRequest {
		room_id: normalize_chat_identifier("roomId", &request.room_id)?,
		anchor: match request.anchor {
			ChatPageAnchor::After { cursor: after } => {
				ChatPageAnchor::After { cursor: cursor(after)? }
			},
			ChatPageAnchor::Before { cursor: before } => {
				ChatPageAnchor::Before { cursor: cursor(before)? }
			},
		},
		page_size: check_chat_page_size(request.page_size, CHAT_MESSAGES_PAGE_MAX)?,
	})
}

/// Validate a question about participants a host asks.
pub fn validate_chat_participants_query(
	request: ProductChatParticipantsQueryRequest,
) -> Result<ProductChatParticipantsQueryRequest, ChatFieldError> {
	Ok(ProductChatParticipantsQueryRequest {
		room_id: normalize_chat_identifier("roomId", &request.room_id)?,
		query: match request.query {
			ChatParticipantsQuery::Count => ChatParticipantsQuery::Count,
			ChatParticipantsQuery::Page { cursor, page_size } => ChatParticipantsQuery::Page {
				cursor: cursor.map(|cursor| validate_chat_cursor("cursor", cursor)).transpose()?,
				page_size: check_chat_page_size(page_size, CHAT_PARTICIPANTS_PAGE_MAX)?,
			},
			ChatParticipantsQuery::ByIds { participant_ids } => {
				check_chat_count("participantIds", &participant_ids, CHAT_PARTICIPANT_IDS_MAX)?;
				ChatParticipantsQuery::ByIds {
					participant_ids: participant_ids
						.iter()
						.map(|id| normalize_chat_identifier("participantIds", id))
						.collect::<Result<_, _>>()?,
				}
			},
		},
	})
}

fn check_chat_page_size(page_size: u32, max: u32) -> Result<u32, ChatFieldError> {
	if !(1..=max).contains(&page_size) {
		return Err(ChatFieldError::OutOfRange { field: "pageSize", min: 1, max: max.into() });
	}
	Ok(page_size)
}

/// Validate a page of messages a product answers for a page of `page_size`.
pub fn validate_chat_messages_page(
	page: ProductChatMessagesQueryItem,
	page_size: u32,
) -> Result<ProductChatMessagesQueryItem, ChatFieldError> {
	check_chat_count("messages", &page.messages, page_size as usize)?;
	Ok(ProductChatMessagesQueryItem {
		messages: page.messages.into_iter().map(validate_chat_message).collect::<Result<_, _>>()?,
		next: page.next.map(|next| validate_chat_cursor("next", next)).transpose()?,
	})
}

/// Validate who reacted to a message, as a product answers it.
pub fn validate_chat_reactions_answer(
	answer: ProductChatReactionsQueryItem,
) -> Result<ProductChatReactionsQueryItem, ChatFieldError> {
	Ok(ProductChatReactionsQueryItem {
		reactions: answer
			.reactions
			.into_iter()
			.map(|reaction| {
				Ok(ChatReactionDetail {
					participant_id: normalize_chat_identifier(
						"participantId",
						&reaction.participant_id,
					)?,
					emoji: validate_chat_emoji("emoji", &reaction.emoji)?,
				})
			})
			.collect::<Result<_, ChatFieldError>>()?,
	})
}

/// Validate the participants a product answers.
pub fn validate_chat_participants_answer(
	answer: ProductChatParticipantsQueryItem,
) -> Result<ProductChatParticipantsQueryItem, ChatFieldError> {
	let entries = |participants: Vec<ChatParticipantEntry>| {
		check_chat_count("participants", &participants, CHAT_PARTICIPANTS_PAGE_MAX as usize)?;
		participants
			.into_iter()
			.map(validate_chat_participant_entry)
			.collect::<Result<Vec<_>, _>>()
	};
	Ok(match answer {
		ProductChatParticipantsQueryItem::Count { count } => {
			ProductChatParticipantsQueryItem::Count { count }
		},
		ProductChatParticipantsQueryItem::Page { participants, next } => {
			ProductChatParticipantsQueryItem::Page {
				participants: entries(participants)?,
				next: next.map(|next| validate_chat_cursor("next", next)).transpose()?,
			}
		},
		ProductChatParticipantsQueryItem::Resolved { participants } => {
			ProductChatParticipantsQueryItem::Resolved { participants: entries(participants)? }
		},
	})
}

/// Token bucket for each `(product, room)` pair: at most `per_second` calls in
/// one second, refilled at the same rate.
pub struct ChatRateLimiter {
	per_second: u32,
	buckets: std::sync::Mutex<std::collections::HashMap<(String, String), ChatRateBucket>>,
}

struct ChatRateBucket {
	/// Tokens left, in thousandths of a token.
	milli_tokens: u64,
	/// Milliseconds since the Unix epoch of the last refill.
	refilled_at_ms: u64,
}

impl ChatRateLimiter {
	/// A limiter that accepts `per_second` calls for each room in one second.
	pub fn new(per_second: u32) -> Self {
		Self { per_second, buckets: Default::default() }
	}

	/// Take one token for `room_id` of `product_id` at `now_ms`, milliseconds
	/// since the Unix epoch. `false` when the room used its budget.
	pub fn try_acquire(&self, product_id: &str, room_id: &str, now_ms: u64) -> bool {
		let capacity = u64::from(self.per_second) * 1000;
		let mut buckets = self.buckets.lock().expect("chat rate limiter mutex poisoned");
		let bucket = buckets
			.entry((product_id.to_string(), room_id.to_string()))
			.or_insert(ChatRateBucket { milli_tokens: capacity, refilled_at_ms: now_ms });
		let elapsed_ms = now_ms.saturating_sub(bucket.refilled_at_ms);
		bucket.milli_tokens = capacity
			.min(bucket.milli_tokens.saturating_add(elapsed_ms * u64::from(self.per_second)));
		bucket.refilled_at_ms = bucket.refilled_at_ms.max(now_ms);
		if bucket.milli_tokens < 1000 {
			return false;
		}
		bucket.milli_tokens -= 1000;
		true
	}

	/// Drop the budget of a deleted room.
	pub fn forget(&self, product_id: &str, room_id: &str) {
		let mut buckets = self.buckets.lock().expect("chat rate limiter mutex poisoned");
		buckets.remove(&(product_id.to_string(), room_id.to_string()));
	}
}

/// Scheme a URL parser would resolve, with the characters parsers ignore
/// removed so `java\tscript:` and a leading NUL cannot hide one.
fn icon_scheme(candidate: &str) -> Option<String> {
	let stripped: String = candidate
		.chars()
		.filter(|character| !character.is_whitespace() && *character != '\u{0}')
		.collect();
	let (scheme, _) = stripped.split_once(':')?;
	if scheme.is_empty() ||
		!scheme
			.chars()
			.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
	{
		return None;
	}
	Some(scheme.to_ascii_lowercase())
}

/// Whether an inline image declares an allowed media type. The media type is
/// read the way a data-URL processor reads it: whitespace-insensitive.
fn is_allowed_icon_data_url(candidate: &str) -> bool {
	let Some(rest) = candidate
		.char_indices()
		.find(|(_, c)| *c == ':')
		.map(|(index, _)| &candidate[index + 1..])
	else {
		return false;
	};
	let media_type: String = rest
		.split(&[',', ';'][..])
		.next()
		.unwrap_or_default()
		.chars()
		.filter(|character| !character.is_whitespace())
		.collect::<String>()
		.to_ascii_lowercase();
	ALLOWED_ICON_DATA_TYPES.contains(&media_type.as_str())
}

/// Invisible characters an identifier must not carry. A display name keeps
/// these: ZWJ builds emoji sequences and ZWNJ is required by Persian.
fn is_identifier_unsafe(character: char) -> bool {
	matches!(
		character,
		'\u{00ad}'                        // soft hyphen
			| '\u{200c}' | '\u{200d}'     // ZWNJ, ZWJ
			| '\u{2060}'..='\u{2064}'     // word joiner, invisible operators
			| '\u{fe00}'..='\u{fe0f}'     // variation selectors
	) || (character.is_whitespace() && character != ' ')
}

/// Line breaks and tabs a message body legitimately carries. A body is written
/// text, so these are content; every other character
/// [`is_display_unsafe`] rejects still applies.
fn is_body_unsafe(character: char) -> bool {
	if matches!(character, '\n' | '\r' | '\t') {
		return false;
	}
	is_display_unsafe(character)
}

/// Tag characters encode the subdivision flags, so a picked reaction keeps
/// them where a display label would not.
fn is_emoji_unsafe(character: char) -> bool {
	if matches!(character, '\u{e0020}'..='\u{e007f}') {
		return false;
	}
	is_display_unsafe(character)
}

/// Control characters and bidi overrides let two distinct values render alike.
fn is_display_unsafe(character: char) -> bool {
	character.is_control() ||
		matches!(
			character,
			'\u{200b}'
				| '\u{061c}'
				| '\u{2028}' | '\u{2029}'   // line and paragraph separators
				| '\u{202a}'..='\u{202e}'
				| '\u{2066}'..='\u{2069}'
				| '\u{feff}'
				| '\u{e0000}'..='\u{e007f}'
		)
}

/// Rejection of a product-supplied chat field.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum ChatFieldError {
	/// The field is required and arrived blank.
	#[display("{field} must not be empty")]
	Empty {
		/// Offending field name.
		field: &'static str,
	},
	/// Two entries resolved to the same value, so neither can be addressed.
	#[display("{field} must not repeat a value")]
	Duplicate {
		/// Offending field name.
		field: &'static str,
	},
	/// The URL carried credentials, which a host would fetch and log with.
	#[display("{field} must not carry credentials")]
	Credentials {
		/// Offending field name.
		field: &'static str,
	},
	/// The field names a path rather than one file. Reported separately from
	/// [`Self::UnsafeCharacter`] because a separator or a parent reference is
	/// neither: a product told its file name carries control characters would
	/// go looking for one that is not there.
	#[display("{field} must name a single file, not a path")]
	PathComponent {
		/// Offending field name.
		field: &'static str,
	},
	/// The field carried more entries than are accepted.
	#[display("{field} must not carry more than {limit} entries")]
	TooMany {
		/// Offending field name.
		field: &'static str,
		/// Largest accepted number of entries.
		limit: usize,
	},
	/// The field exceeded its byte budget.
	#[display("{field} must be at most {limit} bytes")]
	TooLong {
		/// Offending field name.
		field: &'static str,
		/// Accepted maximum.
		limit: usize,
	},
	/// The field carried characters that make values indistinguishable.
	#[display("{field} must not contain control or bidirectional characters")]
	UnsafeCharacter {
		/// Offending field name.
		field: &'static str,
	},
	/// The icon carried a scheme a host must not render.
	#[display("{field} carries a scheme that cannot be rendered")]
	RejectedScheme {
		/// Offending field name.
		field: &'static str,
	},
	/// The field does not have the one length it must have.
	#[display("{field} must be exactly {length} bytes")]
	WrongLength {
		/// Offending field name.
		field: &'static str,
		/// Required length.
		length: usize,
	},
	/// The value is outside its accepted range.
	#[display("{field} must be from {min} to {max}")]
	OutOfRange {
		/// Offending field name.
		field: &'static str,
		/// Smallest accepted value.
		min: u64,
		/// Largest accepted value.
		max: u64,
	},
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), RuntimeConfigValidationError> {
	if value.trim().is_empty() {
		return Err(RuntimeConfigValidationError::EmptyField { field });
	}
	Ok(())
}

/// Runtime config validation error.
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum RuntimeConfigValidationError {
	/// Required string field was empty or whitespace-only.
	#[display("{field} must not be empty")]
	EmptyField {
		/// Field name.
		field: &'static str,
	},
	/// Host icon URL could not be parsed as an absolute HTTPS URL.
	#[display("host_info.icon must be an absolute HTTPS URL: {source}")]
	InvalidHostIcon {
		/// Parse failure.
		source: url::ParseError,
	},
	/// Host icon URL used a non-HTTPS scheme.
	#[display("host_info.icon must use https scheme, got {scheme:?}")]
	InsecureHostIcon {
		/// Actual URL scheme.
		scheme: String,
	},
	/// Pairing deeplink scheme included a URL separator.
	#[display("pairing_deeplink_scheme must not include ://, got {scheme:?}")]
	InvalidDeeplinkScheme {
		/// Actual deeplink scheme value.
		scheme: String,
	},
	/// Product id was not a dotNS or localhost product identifier.
	#[display("product_id must be a dotNS or localhost product identifier, got {product_id:?}")]
	InvalidProductId {
		/// Actual product id value.
		product_id: String,
	},
	/// Network suffix was not a supported dotNS TLD.
	#[display("network_suffix must be a supported dotNS TLD, got {network_suffix:?}")]
	InvalidNetworkSuffix {
		/// Actual network suffix value.
		network_suffix: String,
	},
	/// Product id was longer than [`PRODUCT_ID_MAX_BYTES`] after normalization.
	///
	/// Carries lengths rather than the id, which may be enormous.
	#[display("product_id must be at most {limit} bytes, got {actual}")]
	ProductIdTooLong {
		/// Accepted maximum, in bytes.
		limit: usize,
		/// Normalized length, in bytes.
		actual: usize,
	},
}

const PRODUCT_STORAGE_KEY_PREFIX: &str = "truapi:product-storage:v1:";

/// Decoded product scope and product-owned key used by [`ProductStorage`].
///
/// The string representation remains the host callback ABI. Native hosts that
/// need separate backing stores can decode it without duplicating the wire
/// format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductStorageKey {
	product_id: String,
	key: String,
}

impl ProductStorageKey {
	/// Build a key with a validated, normalized product id.
	pub fn new(
		product_id: &str,
		key: impl Into<String>,
	) -> Result<Self, RuntimeConfigValidationError> {
		Ok(Self { product_id: normalize_product_identifier(product_id)?, key: key.into() })
	}

	/// Decode the opaque key passed through [`ProductStorage`].
	pub fn decode(value: &str) -> Result<Self, String> {
		let remainder = value
			.strip_prefix(PRODUCT_STORAGE_KEY_PREFIX)
			.ok_or_else(|| "product storage key has an unknown format".to_string())?;
		let (length, scoped) = remainder
			.split_once(':')
			.ok_or_else(|| "product storage key is missing its scope length".to_string())?;
		let product_length = length
			.parse::<usize>()
			.map_err(|_| "product storage key has an invalid scope length".to_string())?;
		let product_id = scoped
			.get(..product_length)
			.ok_or_else(|| "product storage key has a truncated product id".to_string())?;
		let separator = scoped
			.as_bytes()
			.get(product_length)
			.copied()
			.ok_or_else(|| "product storage key is missing its key separator".to_string())?;
		if separator != b':' {
			return Err("product storage key has an invalid key separator".to_string());
		}
		let key = scoped
			.get(product_length + 1..)
			.ok_or_else(|| "product storage key splits a UTF-8 character".to_string())?;
		Self::new(product_id, key.to_string()).map_err(|error| error.to_string())
	}

	/// Product identifier owning this storage key.
	pub fn product_id(&self) -> &str {
		&self.product_id
	}

	/// Product-local key without the host scope prefix.
	pub fn key(&self) -> &str {
		&self.key
	}

	/// Encode the stable opaque key used by existing host callbacks.
	pub fn encode(&self) -> String {
		format!(
			"{PRODUCT_STORAGE_KEY_PREFIX}{}:{}:{}",
			self.product_id.len(),
			self.product_id,
			self.key
		)
	}
}

/// Product-scoped key-value storage.
///
/// The core namespaces product keys before calling this trait, and the key
/// names the product that owns the value. Host implementations may treat
/// `key` as opaque in one shared store. A host that separates physical storage
/// by product must decode the owner with [`ProductStorageKey`] on read, since
/// that owner is another product on a granted foreign read.
/// Storage errors are pinned to `v01` rather than taken from `truapi::latest`.
/// The read error gained a cross-product refusal in v0.2 that the core decides
/// before it ever calls a host, so a host has no way to produce it and should
/// not have to match on it.
#[async_trait]
pub trait ProductStorage: Send + Sync {
	/// Read a value by key.
	///
	/// `key` belongs to the calling product, or to another product the core
	/// has already found granting the caller read access in its manifest. The
	/// core refuses every other foreign read itself, so a host is never asked
	/// to enforce a grant and has no variant for one. Writes and clears always
	/// carry the calling product's own keys.
	async fn read(
		&self,
		key: String,
	) -> Result<Option<Vec<u8>>, truapi::v01::HostLocalStorageReadError>;

	/// Write a value to a key.
	async fn write(
		&self,
		key: String,
		value: Vec<u8>,
	) -> Result<(), truapi::v01::HostLocalStorageReadError>;

	/// Clear a value at a key.
	async fn clear(&self, key: String) -> Result<(), truapi::v01::HostLocalStorageReadError>;

	/// Emit `key`'s current value, then each later change from any of the
	/// product's runtimes. `key` is namespaced exactly as [`Self::read`] takes
	/// it.
	///
	/// Reporting a write that left the bytes unchanged is allowed: the core
	/// drops an item repeating the value it last delivered, so the product
	/// sees only real changes whether or not a host filters them itself.
	fn subscribe_storage(
		&self,
		key: String,
	) -> BoxStream<'static, Result<HostLocalStorageChangeItem, GenericError>>;
}

/// Open URLs in the system browser. Input is already trimmed, categorized,
/// and (where needed) normalized by the core; the host implementation only
/// needs to hand the URL to the OS URL handler.
#[async_trait]
pub trait Navigation: Send + Sync {
	/// Open the given URL in the system browser.
	async fn navigate_to(&self, url: String) -> Result<(), HostNavigateToError>;
}

/// Deliver push notifications.
#[async_trait]
pub trait Notifications: Send + Sync {
	/// Schedule or immediately display the given notification and return the
	/// host-assigned id.
	async fn push_notification(
		&self,
		notification: HostPushNotificationRequest,
	) -> Result<HostPushNotificationResponse, GenericError>;

	/// Cancel a notification by id. Idempotent: cancelling an already-fired or
	/// unknown id still returns `Ok(())`.
	async fn cancel_notification(&self, id: u32) -> Result<(), GenericError> {
		let _ = id;
		Ok(())
	}
}

/// User decision including how long an authorization should last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PermissionDecision {
	/// Authorize the next operation without saving a durable grant.
	AllowOnce,
	/// Save an authorization for future operations.
	AllowAlways,
	/// Save a refusal of the requested permission.
	Deny,
}

/// Permission prompts. Device permissions (camera, mic, NFC, ...) are separate
/// from remote permissions (domain access, chain submit, ...), so the platform
/// surface mirrors that split.
#[async_trait]
pub trait Permissions: Send + Sync {
	/// Prompt the user for a device-level permission `product` requested.
	async fn device_permission(
		&self,
		product: &ProductContext,
		request: HostDevicePermissionRequest,
	) -> Result<PermissionDecision, GenericError>;

	/// Prompt the user for a remote permission bundle `product` requested.
	async fn remote_permission(
		&self,
		product: &ProductContext,
		request: RemotePermissionRequest,
	) -> Result<PermissionDecision, GenericError>;
}

/// Permission request whose authorization status can be inspected or updated
/// by host administration UI.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PermissionAuthorizationRequest {
	/// Device-level permission such as camera, microphone, or location.
	Device(HostDevicePermissionRequest),
	/// Remote/product-scoped permission such as chain submit or HTTP access.
	Remote(RemotePermissionRequest),
	/// Product-scoped permission to disclose the user's primary identity.
	IdentityDisclosure,
	/// Product-scoped permission to access another product's account context.
	AccountAccess {
		/// Product whose account context may be accessed.
		target_product_id: String,
	},
	/// Product-scoped approval to make one kind of signature with another
	/// product's accounts. It is held in memory only and ends when the product
	/// execution closes.
	CrossProductSignature {
		/// Product whose accounts sign.
		target_product_id: String,
		/// Kind of signature the user approved.
		kind: CrossProductSignatureKind,
	},
}

/// A kind of signature a product makes with another product's account.
///
/// The user approves each kind separately: approving one never lets another
/// through without a prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum CrossProductSignatureKind {
	/// `sign_payload`.
	Payload,
	/// `create_transaction` naming no contacts.
	Transaction,
	/// Watermarked `sign_raw`.
	Raw,
	/// A Statement Store product proof.
	StatementProof,
}

/// Authorization status for a permission request.
///
/// `NotDetermined` means the core has no saved or one-use answer and will prompt the
/// host the next time the product requests this permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum PermissionAuthorizationStatus {
	/// No persisted authorization exists.
	NotDetermined,
	/// Access is denied.
	Denied,
	/// Access is authorized.
	Authorized,
}

/// Core-owned administration API exposed to host UI.
///
/// Hosts call this surface to drive global runtime actions or inspect/update
/// core-owned state without going through a product-scoped TrUAPI request.
#[async_trait]
pub trait CoreAdmin: Send + Sync {
	/// Best-effort logout/disconnect. Clears the active session and emits the
	/// resulting auth state transition.
	async fn disconnect_session(&self) -> Result<(), GenericError>;

	/// Read a stored permission authorization status without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	async fn get_permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
	) -> Result<PermissionAuthorizationStatus, GenericError>;

	/// Read stored permission authorization statuses without prompting.
	///
	/// A device capability also resolves the host application's OS gate, so an
	/// OS refusal reads as `Denied` whatever is stored. Remote,
	/// identity-disclosure and account-access decisions have no OS gate.
	///
	/// Results are returned in the same order as `requests`.
	async fn get_permission_authorization_statuses(
		&self,
		requests: Vec<PermissionAuthorizationRequest>,
	) -> Result<Vec<PermissionAuthorizationStatus>, GenericError>;

	/// Update a stored permission authorization status. `NotDetermined` clears
	/// the stored value so the next product request prompts again.
	async fn set_permission_authorization_status(
		&self,
		request: PermissionAuthorizationRequest,
		status: PermissionAuthorizationStatus,
	) -> Result<(), GenericError>;

	/// Read the active session's X25519 chat identity private key, for hosts
	/// that run their own P2P chat channel for the paired identity.
	///
	/// The wallet derives this key from the identity root and shares it during
	/// pairing; the core retains it verbatim, because a value derived
	/// host-side would address an identity no existing peer can reach. `None`
	/// when no session is active.
	///
	/// Deliberately not on [`SessionUiInfo`]: that projection rides every
	/// [`AuthState`] broadcast to all registered [`AuthPresenter`]s, so a
	/// secret placed there would reach hosts that never asked for it.
	async fn get_session_chat_identity_key(&self) -> Result<Option<Bytes32>, GenericError>;

	/// Read the active session's expanded sr25519 statement-store secret, for
	/// hosts that run their own statement-store traffic. 64 bytes.
	///
	/// The key behind [`SessionUiInfo::device_statement_account_id`]: signing
	/// with anything else produces statements no allowance covers. `None`
	/// without an active pairing-host session; a signing host has no pairing
	/// proposal of its own.
	///
	/// Deliberately not on [`SessionUiInfo`], for the reason given on
	/// [`Self::get_session_chat_identity_key`].
	async fn get_device_statement_key(&self) -> Result<Option<Vec<u8>>, GenericError>;

	/// Read this device's X25519 encryption secret, for hosts that run device
	/// sync against the peer's [`SessionUiInfo::device_enc_public_key`].
	///
	/// Generated and persisted on first read, so the returned key is stable for
	/// the install and matches the public key peers were told to address.
	async fn get_device_encryption_key(&self) -> Result<Bytes32, GenericError>;

	/// Read `product_id`'s hard-subtree public key, so a host can name the
	/// account a review will sign with instead of showing a bare derivation
	/// path.
	///
	/// Resolves from the memory cache, then the persisted slot, then the
	/// Account Holder. A pairing host reaching the wallet sends an SSO request,
	/// which answers without prompting the user, though it can wake the phone.
	/// A signing host derives locally and never waits.
	///
	/// `timeout_ms` bounds that wait, and exceeding it is an error rather than
	/// `None`. The underlying wait has no deadline of its own, so a host
	/// calling this while drawing a review should pass a timeout it is willing
	/// to block for. `None` uses a default sized for a product awaiting a
	/// signature, which is far too long to hold a render.
	///
	/// `None` means no active session. Derive account public keys from the
	/// answer with `deriveProductAccountPublicKey`.
	async fn get_product_subtree_public_key(
		&self,
		product_id: String,
		timeout_ms: Option<u32>,
	) -> Result<Option<Bytes32>, GenericError>;
}

/// Pairing-host-only administration API exposed to host UI.
#[async_trait]
pub trait PairingHostAdmin: Send + Sync {
	/// Cancel any in-flight pairing request.
	fn cancel_pairing(&self);

	/// Notify the core that the persisted auth-session blob may have changed.
	///
	/// The host owns persistence and change detection. The pairing core owns
	/// decoding that blob into live `SessionState` / `AuthState`.
	fn notify_session_store_changed(&self);
}

/// One chain a host serves: a protocol chain role mapped to the concrete
/// chain of the host's configured environment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostChainEntry {
	/// Protocol role this entry answers for.
	pub identifier: ChainIdentifier,
	/// Genesis hash identifying the chain in all chain-scoped calls.
	pub genesis_hash: Bytes32,
}

/// The chain set a host serves: its environment plus one entry per chain role.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostChainSet {
	/// Ecosystem the host is configured for, e.g. "polkadot", "paseo".
	pub network: String,
	/// Chains this host serves, keyed by protocol role.
	pub chains: Vec<HostChainEntry>,
}

/// Feature-support probing. The host answers whether it can service a given
/// capability (currently scoped to per-chain support).
#[async_trait]
pub trait Features: Send + Sync {
	/// Report whether the requested feature is supported.
	async fn feature_supported(
		&self,
		request: HostFeatureSupportedRequest,
	) -> Result<HostFeatureSupportedResponse, GenericError>;

	/// Enumerate the chains this host serves (RFC 0026). The core resolves
	/// `get_chain_info` requests against the returned set.
	async fn supported_chains(&self) -> Result<HostChainSet, GenericError>;
}

/// Core-owned host-private storage slots. Products never address these slots;
/// the host chooses the backing store for each slot.
///
/// Storage is host-local; `storage.md` records the current status quo:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/storage.md?plain=1#L1-L7>
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum CoreStorageKey {
	/// Opaque SSO/auth session blob.
	#[codec(index = 0)]
	AuthSession,
	/// Pairing device identity used during SSO flows.
	PairingDeviceIdentity,
	/// Persisted authorization for one product-scoped permission request.
	PermissionAuthorization {
		/// Product whose permission decision is being stored.
		product_id: String,
		/// Permission request whose authorization is being stored.
		request: PermissionAuthorizationRequest,
	},
	/// Persisted allowance-slot keys for one paired SSO session.
	AllowanceKeys {
		/// Stable host-derived SSO session id.
		session_id: String,
	},
	/// Last processed SSO pairing response statement for the pairing device.
	LastProcessedPairingStatement,
	/// Legacy unscoped RFC-0010 AutoSigning secret. Core only addresses this
	/// slot to reject and erase pre-scoping entries.
	AutoSigningKey {
		/// Product whose hard subtree the legacy secret controlled.
		product_id: String,
	},
	/// Wallet-bound RFC-0010 AutoSigning capabilities for the active pairing.
	AutoSigningKeys,
	/// Wallet-bound RFC-0024 ring-VRF registry snapshot.
	#[codec(index = 7)]
	RingVrfRegistry {
		/// Root account public key identifying the wallet that owns the registry.
		root_public_key: [u8; 32],
	},
	/// Statement-store allowance targets the signing host keeps renewed.
	#[codec(index = 8)]
	StatementRenewalTargets,
	/// This device's long-lived X25519 encryption secret, advertised to peers
	/// as the device encryption public key. Random rather than identity-derived
	/// so devices restoring one identity stay individually addressable.
	///
	/// Hosts must back this slot with storage scoped to the install, outliving
	/// logout and any per-user namespacing: once it changes, peers addressing
	/// the previous key can no longer reach this device.
	#[codec(index = 9)]
	DeviceEncryptionKey,
	/// One product's hard-subtree public key, as the Account Holder answered it
	/// for this paired session. Product account is a hard derivation, so the
	/// answer is fixed for the pair and read back instead of re-asking the
	/// wallet on every launch.
	///
	/// The value is the 32-byte key with no framing, so a host can derive
	/// product account addresses from the slot it already stores. These are
	/// public keys: every address derived from them already appears on the
	/// reviews the host draws.
	#[codec(index = 10)]
	ProductSubtree {
		/// Stable host-derived SSO session id.
		session_id: String,
		/// Product whose hard subtree this key roots.
		product_id: String,
	},
	/// Signing-host request replay state for one wallet and pairing peer.
	///
	/// The value is a versioned, bounded replay ledger owned by the core.
	#[codec(index = 11)]
	SsoResponderRequestLedger {
		/// Root public key of the wallet that served the requests.
		root_public_key: [u8; 32],
		/// Pairing peer's statement-store account id.
		peer_statement_account_id: [u8; 32],
		/// Pairing peer's X25519 public key.
		peer_encryption_public_key: [u8; 32],
	},
	/// Cached root manifest of one product, as published to dotNS.
	///
	/// The value carries the manifest JSON alongside the time it was read. The
	/// core honours it for a bounded lifetime, which is what makes a revoked
	/// trust grant eventually take effect.
	#[codec(index = 12)]
	ProductManifest {
		/// Product whose manifest was cached, normalized.
		product_id: String,
	},
	/// Funding sessions: every live one plus a bounded tail of settled ones,
	/// as one versioned SCALE blob.
	#[codec(index = 13)]
	FundingSessions,
	/// Funding sessions or session pack blobs the core could not read, as a SCALE list of the
	/// raw blobs, newest last, so a stored session is never overwritten
	/// unread and a later bad blob does not replace an earlier one. Kept for
	/// diagnosis only: the core never reads them back.
	#[codec(index = 17)]
	FundingSessionsUnreadable,
	/// Recorded funding sessions compacted out of the funding sessions blob,
	/// as one versioned SCALE blob. Never trimmed.
	#[codec(index = 18)]
	FundingSessionPack,
	/// The wake watches of every product and the key this device registers
	/// with its push relay. The key authenticates wake pushes, so hosts must
	/// keep this slot private to the install.
	#[codec(index = 19)]
	PushWakeRegistry,
	/// A file this device uploaded to HOP for a contact of the chat product.
	/// The core never acks such a file, so showing one's own photo cannot
	/// drop it from the node before the contact claims it. The value is empty.
	#[codec(index = 20)]
	HopUpload {
		/// The identifier of the file's root entry.
		identifier: [u8; 32],
	},
	/// The X25519 secret the native chat of this install used as its device
	/// key, which the chat product keeps using so contacts still reach this
	/// device. The host writes it once, through the core, when it moves its
	/// native chat to the chat product; absent on every other install.
	#[codec(index = 21)]
	ChatDeviceKey,
}

/// Stable metadata describing one strictly decoded [`CoreStorageKey`].
///
/// `kind` is the Rust variant name and is part of the host embedding contract.
/// `product_id` is present only for keys whose storage slot is directly
/// product-indexed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreStorageKeyDescription {
	/// Stable storage-key variant name.
	pub kind: &'static str,
	/// Product that owns this exact slot, when the key is product-indexed.
	pub product_id: Option<String>,
}

/// Failure to decode exactly one [`CoreStorageKey`].
#[derive(Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error)]
pub enum CoreStorageKeyDescriptionError {
	/// The bytes are not a valid SCALE-encoded key.
	#[display("invalid CoreStorageKey encoding")]
	InvalidEncoding,
	/// A valid key was followed by additional bytes.
	#[display("CoreStorageKey encoding contains trailing bytes")]
	TrailingBytes,
}

/// Strictly decode one SCALE-encoded [`CoreStorageKey`] and return stable
/// metadata suitable for choosing host-private storage policy.
pub fn describe_core_storage_key(
	encoded: &[u8],
) -> Result<CoreStorageKeyDescription, CoreStorageKeyDescriptionError> {
	let mut input = encoded;
	let key = CoreStorageKey::decode(&mut input)
		.map_err(|_| CoreStorageKeyDescriptionError::InvalidEncoding)?;
	if !input.is_empty() {
		return Err(CoreStorageKeyDescriptionError::TrailingBytes);
	}
	let (kind, product_id) = match key {
		CoreStorageKey::AuthSession => ("AuthSession", None),
		CoreStorageKey::PairingDeviceIdentity => ("PairingDeviceIdentity", None),
		CoreStorageKey::PermissionAuthorization { product_id, .. } => {
			("PermissionAuthorization", Some(product_id))
		},
		CoreStorageKey::AllowanceKeys { .. } => ("AllowanceKeys", None),
		CoreStorageKey::LastProcessedPairingStatement => ("LastProcessedPairingStatement", None),
		CoreStorageKey::AutoSigningKey { product_id } => ("AutoSigningKey", Some(product_id)),
		CoreStorageKey::ProductSubtree { product_id, .. } => ("ProductSubtree", Some(product_id)),
		CoreStorageKey::AutoSigningKeys => ("AutoSigningKeys", None),
		CoreStorageKey::RingVrfRegistry { .. } => ("RingVrfRegistry", None),
		CoreStorageKey::StatementRenewalTargets => ("StatementRenewalTargets", None),
		CoreStorageKey::DeviceEncryptionKey => ("DeviceEncryptionKey", None),
		CoreStorageKey::SsoResponderRequestLedger { .. } => ("SsoResponderRequestLedger", None),
		CoreStorageKey::ProductManifest { product_id } => ("ProductManifest", Some(product_id)),
		CoreStorageKey::FundingSessions => ("FundingSessions", None),
		CoreStorageKey::FundingSessionsUnreadable => ("FundingSessionsUnreadable", None),
		CoreStorageKey::FundingSessionPack => ("FundingSessionPack", None),
		CoreStorageKey::PushWakeRegistry => ("PushWakeRegistry", None),
		CoreStorageKey::HopUpload { .. } => ("HopUpload", None),
		CoreStorageKey::ChatDeviceKey => ("ChatDeviceKey", None),
	};
	Ok(CoreStorageKeyDescription { kind, product_id })
}

impl CoreStorageKey {
	/// Persisted authorization key for one product-scoped device permission.
	pub fn device_permission_authorization(
		product_id: &str,
		permission: &HostDevicePermissionRequest,
	) -> Self {
		Self::PermissionAuthorization {
			product_id: product_id.to_string(),
			request: PermissionAuthorizationRequest::Device(*permission),
		}
	}

	/// Persisted authorization key for one product-scoped remote permission.
	pub fn remote_permission_authorization(
		product_id: &str,
		request: &RemotePermissionRequest,
	) -> Self {
		Self::PermissionAuthorization {
			product_id: product_id.to_string(),
			request: PermissionAuthorizationRequest::Remote(canonical_remote_request(request)),
		}
	}

	/// Persisted authorization key for a single domain pattern inside a
	/// product's remote-access grant.
	///
	/// A product may request several domains at once, but enforcement asks
	/// about one host at a time, so each pattern gets its own slot: a
	/// multi-domain grant is stored as one key per pattern and stays visible to
	/// a later single-host lookup. The key is a one-element
	/// [`RemotePermission::Remote`] set, so this shares the encoding — and the
	/// [`normalize_remote_domain`] canonicalization — of the bundle form.
	pub fn remote_domain_authorization(product_id: &str, domain: &str) -> Self {
		Self::remote_permission_authorization(
			product_id,
			&RemotePermissionRequest {
				permission: RemotePermission::Remote { domains: vec![domain.to_string()] },
			},
		)
	}

	/// Persisted authorization key for product-scoped identity disclosure.
	pub fn identity_disclosure_authorization(product_id: &str) -> Self {
		Self::PermissionAuthorization {
			product_id: product_id.to_string(),
			request: PermissionAuthorizationRequest::IdentityDisclosure,
		}
	}

	/// Persisted authorization key for one product accessing another product's
	/// account context.
	pub fn account_access_authorization(product_id: &str, target_product_id: &str) -> Self {
		Self::PermissionAuthorization {
			product_id: product_id.to_string(),
			request: PermissionAuthorizationRequest::AccountAccess {
				target_product_id: target_product_id.to_string(),
			},
		}
	}
}

/// Canonical storage form for one remote-access domain pattern.
///
/// Both ends of a domain lookup have to agree byte for byte — the pattern a
/// grant is keyed under and the host enforcement derives from a live URL — so
/// both run this one rule: IDNA ASCII form, lower-cased, trailing root dot
/// dropped, leading `*.` wildcard marker preserved. Without it a trailing-dot
/// FQDN or a non-ASCII host would open a second slot for the same site and
/// prompt twice.
///
/// Input the URL host parser rejects as a domain falls back to an NFC-folded
/// lowercase form, so an unusual pattern still keys consistently rather than
/// being dropped.
pub fn normalize_remote_domain(domain: &str) -> String {
	let trimmed = domain.trim();
	let (wildcard, rest) = match trimmed.strip_prefix("*.") {
		Some(rest) => ("*.", rest),
		None => ("", trimmed),
	};
	let without_root_dot = rest.strip_suffix('.').unwrap_or(rest);
	let normalized = match Host::parse(without_root_dot) {
		Ok(host) => host.to_string(),
		Err(_) => without_root_dot.nfc().collect::<String>().to_lowercase(),
	};
	format!("{wildcard}{normalized}")
}

/// Accepts exact hosts, `*`, and domain wildcards with at least two suffix labels.
/// IP addresses cannot be wildcard suffixes.
pub fn is_valid_remote_domain_pattern(domain: &str) -> bool {
	let normalized = normalize_remote_domain(domain);
	if normalized == "*" {
		return true;
	}
	let wildcard = normalized.starts_with("*.");
	let host = normalized.strip_prefix("*.").unwrap_or(&normalized);
	match Host::parse(host) {
		Ok(Host::Domain(domain)) => {
			!domain.contains('*') &&
				domain.split('.').all(|label| !label.is_empty()) &&
				(!wildcard || domain.contains('.'))
		},
		Ok(_) => !wildcard,
		Err(_) => false,
	}
}

/// Matching domain patterns, ordered for most-specific permission decisions.
/// Wildcards cover descendants at every depth, excluding one-label parents
/// such as `*.com`. Bare parents are omitted so a grant for `example.com`
/// never grants access to its subdomains.
pub fn remote_domain_candidates(host: &str) -> Vec<String> {
	let normalized = normalize_remote_domain(host);
	if matches!(Host::parse(&normalized), Ok(Host::Ipv4(_) | Host::Ipv6(_))) {
		return vec![normalized, "*".to_string()];
	}
	let mut candidates = vec![normalized.clone()];
	let mut descendant = normalized.as_str();
	while let Some((_label, parent)) = descendant.split_once('.') {
		if !parent.contains('.') {
			break;
		}
		candidates.push(format!("*.{parent}"));
		descendant = parent;
	}
	candidates.push("*".to_string());
	candidates.dedup();
	candidates
}

fn canonical_remote_request(request: &RemotePermissionRequest) -> RemotePermissionRequest {
	let permission = match &request.permission {
		RemotePermission::Remote { domains } => {
			// A logically-identical bundle requested with different casing,
			// spelling or duplicate entries must canonicalize to one key (no
			// spurious re-prompt), under the same rule enforcement applies to a
			// single host.
			let mut canonical: Vec<String> =
				domains.iter().map(|domain| normalize_remote_domain(domain)).collect();
			canonical.sort();
			canonical.dedup();
			RemotePermission::Remote { domains: canonical }
		},
		other => other.clone(),
	};
	RemotePermissionRequest { permission }
}

#[cfg(test)]
mod tests {
	use super::*;

	fn signing_host_config(
		network_suffix: &str,
	) -> Result<SigningHostConfig, RuntimeConfigValidationError> {
		SigningHostConfig::new(
			HostInfo {
				name: "Test host".to_string(),
				icon: None,
				version: None,
				platform: HostPlatform::Unknown,
			},
			PlatformInfo::default(),
			[0; 32],
			[1; 32],
			[2; 32],
			network_suffix.to_string(),
		)
	}

	#[test]
	fn a_signing_host_is_configured_for_one_dotns_tld() {
		for tld in DOTNS_TLDS {
			let config = signing_host_config(tld).expect("a known TLD is a valid suffix");
			assert_eq!(config.network_suffix, *tld);
		}

		assert_eq!(
			signing_host_config(""),
			Err(RuntimeConfigValidationError::EmptyField { field: "network_suffix" })
		);
		for malformed in [
			"pasoe",
			"unknown",
			".paseo",
			"peopl.paseo",
			"Paseo",
			"pas eo",
			"a-b",
			"abcdefghijklmnopq",
		] {
			assert_eq!(
				signing_host_config(malformed),
				Err(RuntimeConfigValidationError::InvalidNetworkSuffix {
					network_suffix: malformed.to_string()
				}),
				"{malformed:?} must be rejected"
			);
		}
	}

	fn file_with_url(url: &str) -> ChatMessageContent {
		ChatMessageContent::File(ChatFile {
			url: url.to_string(),
			file_name: "report.pdf".to_string(),
			mime_type: "application/pdf".to_string(),
			size_bytes: 1,
			text: None,
		})
	}

	#[test]
	fn a_message_url_is_screened_like_an_icon() {
		// The icon field already rejects these. A file card is fetched or
		// opened the same way, so the same allowlist applies.
		for hostile in [
			"javascript:alert(document.cookie)",
			"file:///etc/passwd",
			"content://com.host.provider/secret",
			"data:image/svg+xml,<svg onload=alert(1)>",
		] {
			assert_eq!(
				validate_chat_message_content(file_with_url(hostile)),
				Err(ChatFieldError::RejectedScheme { field: "url" }),
				"{hostile} must be rejected"
			);
		}

		// Characters a URL parser drops are rejected before it runs, so the
		// string a host renders is the string the scheme check ran against.
		for smuggled in [
			"java\tscript:alert(1)",
			"\u{0}javascript:alert(1)",
			"https:/\t/evil.invalid/x",
			"https://\u{200b}evil.invalid/x",
			"https://example.invalid/\u{202e}gpj.exe",
		] {
			assert_eq!(
				validate_chat_message_content(file_with_url(smuggled)),
				Err(ChatFieldError::UnsafeCharacter { field: "url" }),
				"{smuggled} must be rejected"
			);
		}

		// What reaches the host is what the parser resolved.
		assert_eq!(
			validate_chat_message_content(file_with_url("https://example.invalid")),
			Ok(file_with_url("https://example.invalid/"))
		);
	}

	#[test]
	fn a_file_name_cannot_reverse_its_own_extension() {
		// A bidi override renders `invoice<RLO>gnp.exe` as `invoiceexe.png`
		// on the download affordance the host draws.
		let spoofed = ChatMessageContent::File(ChatFile {
			url: "https://example.invalid/f".to_string(),
			file_name: "invoice\u{202e}gnp.exe".to_string(),
			mime_type: "application/pdf".to_string(),
			size_bytes: 1,
			text: None,
		});
		assert_eq!(
			validate_chat_message_content(spoofed),
			Err(ChatFieldError::UnsafeCharacter { field: "fileName" })
		);
	}

	#[test]
	fn message_content_is_bounded_by_count_and_by_bytes() {
		let too_many = ChatMessageContent::Actions(ChatActions {
			text: None,
			actions: (0..CHAT_ACTIONS_MAX + 1)
				.map(|index| ChatAction { action_id: format!("a{index}"), title: "go".to_string() })
				.collect(),
			layout: truapi::latest::ChatActionLayout::Column,
		});
		assert_eq!(
			validate_chat_message_content(too_many),
			Err(ChatFieldError::TooMany { field: "actions", limit: CHAT_ACTIONS_MAX })
		);

		let too_long = ChatMessageContent::Text { text: "x".repeat(CHAT_BODY_MAX_BYTES + 1) };
		assert_eq!(
			validate_chat_message_content(too_long),
			Err(ChatFieldError::TooLong { field: "text", limit: CHAT_BODY_MAX_BYTES })
		);

		let too_much_media = ChatMessageContent::RichText(ChatRichText {
			text: None,
			media: (0..CHAT_MEDIA_MAX + 1)
				.map(|_| ChatMedia { url: "https://example.invalid/m".to_string() })
				.collect(),
		});
		assert_eq!(
			validate_chat_message_content(too_much_media),
			Err(ChatFieldError::TooMany { field: "media", limit: CHAT_MEDIA_MAX })
		);
	}

	#[test]
	fn a_message_body_carries_the_text_a_person_typed() {
		// A chat message is written text: line breaks and tabs are content,
		// and the bytes must survive so a product can echo-compare them.
		let markdown = "# Report\n\n| a | b |\n| - | - |\n\tindented\n";
		assert_eq!(
			validate_chat_message_content(ChatMessageContent::Text { text: markdown.to_string() }),
			Ok(ChatMessageContent::Text { text: markdown.to_string() })
		);

		// Neither trimmed nor NFC-normalized.
		let unnormalized = "  cafe\u{301}  ";
		assert_eq!(
			validate_chat_message_content(ChatMessageContent::Text {
				text: unnormalized.to_string(),
			}),
			Ok(ChatMessageContent::Text { text: unnormalized.to_string() })
		);

		// The bidi and zero-width screen still applies.
		assert_eq!(
			validate_chat_message_content(ChatMessageContent::Text {
				text: "pay \u{202e}yletamitigel".to_string(),
			}),
			Err(ChatFieldError::UnsafeCharacter { field: "text" })
		);
	}

	#[test]
	fn a_reaction_keeps_the_emoji_a_person_picked() {
		// Subdivision flags encode as tag characters, which a display label
		// rejects and a picked glyph must not.
		for emoji in [
			"\u{1f3f4}\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}",
			"\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}",
			"\u{2764}\u{fe0f}",
			"\u{1f1ee}\u{1f1f3}",
		] {
			assert_eq!(
				validate_chat_message_content(ChatMessageContent::Reaction(ChatReaction {
					message_id: "message-1".to_string(),
					emoji: emoji.to_string(),
				})),
				Ok(ChatMessageContent::Reaction(ChatReaction {
					message_id: "message-1".to_string(),
					emoji: emoji.to_string(),
				})),
				"{emoji:?} must survive"
			);
		}
	}

	#[test]
	fn every_screened_field_rejects_its_own_hostile_value() {
		// One case per screen, so removing any single one fails a test.
		let media = ChatMessageContent::RichText(ChatRichText {
			text: None,
			media: vec![ChatMedia { url: "javascript:alert(1)".to_string() }],
		});
		assert_eq!(
			validate_chat_message_content(media),
			Err(ChatFieldError::RejectedScheme { field: "url" })
		);

		let action = ChatMessageContent::Actions(ChatActions {
			text: None,
			actions: vec![ChatAction {
				action_id: "app\u{200d}rove".to_string(),
				title: "Approve".to_string(),
			}],
			layout: truapi::latest::ChatActionLayout::Column,
		});
		assert_eq!(
			validate_chat_message_content(action),
			Err(ChatFieldError::UnsafeCharacter { field: "actionId" })
		);

		let mime = ChatMessageContent::File(ChatFile {
			url: "https://example.invalid/f".to_string(),
			file_name: "f".to_string(),
			mime_type: "text/\u{202e}nialp".to_string(),
			size_bytes: 1,
			text: None,
		});
		assert_eq!(
			validate_chat_message_content(mime),
			Err(ChatFieldError::UnsafeCharacter { field: "mimeType" })
		);

		let custom_type = ChatMessageContent::Custom(ChatCustomMessage {
			message_type: "  ".to_string(),
			payload: Vec::new(),
		});
		assert_eq!(
			validate_chat_message_content(custom_type),
			Err(ChatFieldError::Empty { field: "messageType" })
		);

		let payload = ChatMessageContent::Custom(ChatCustomMessage {
			message_type: "vote".to_string(),
			payload: vec![0; CHAT_CUSTOM_PAYLOAD_MAX_BYTES + 1],
		});
		assert_eq!(
			validate_chat_message_content(payload),
			Err(ChatFieldError::TooLong { field: "payload", limit: CHAT_CUSTOM_PAYLOAD_MAX_BYTES })
		);

		let emoji = ChatMessageContent::Reaction(ChatReaction {
			message_id: "message-1".to_string(),
			emoji: "\u{202e}".to_string(),
		});
		assert_eq!(
			validate_chat_message_content(emoji),
			Err(ChatFieldError::UnsafeCharacter { field: "emoji" })
		);

		// The removal variant screens the same fields as the addition.
		let removed = ChatMessageContent::ReactionRemoved(ChatReaction {
			message_id: "  ".to_string(),
			emoji: "\u{1f3b2}".to_string(),
		});
		assert_eq!(
			validate_chat_message_content(removed),
			Err(ChatFieldError::Empty { field: "messageId" })
		);

		// Every optional body, not just the one `Text` carries.
		let bidi = "pay \u{202e}yletamitigel".to_string();
		let bodies = [
			ChatMessageContent::RichText(ChatRichText {
				text: Some(bidi.clone()),
				media: Vec::new(),
			}),
			ChatMessageContent::Actions(ChatActions {
				text: Some(bidi.clone()),
				actions: Vec::new(),
				layout: truapi::latest::ChatActionLayout::Column,
			}),
			ChatMessageContent::File(ChatFile {
				url: "https://example.invalid/f".to_string(),
				file_name: "f".to_string(),
				mime_type: "text/plain".to_string(),
				size_bytes: 1,
				text: Some(bidi),
			}),
		];
		for body in bodies {
			assert_eq!(
				validate_chat_message_content(body.clone()),
				Err(ChatFieldError::UnsafeCharacter { field: "text" }),
				"{body:?} must screen its body"
			);
		}

		let title = ChatMessageContent::Actions(ChatActions {
			text: None,
			actions: vec![ChatAction {
				action_id: "approve".to_string(),
				title: "Approve\u{202e}".to_string(),
			}],
			layout: truapi::latest::ChatActionLayout::Column,
		});
		assert_eq!(
			validate_chat_message_content(title),
			Err(ChatFieldError::UnsafeCharacter { field: "title" })
		);
	}

	#[test]
	fn an_icon_is_screened_and_resolved_like_a_message_url() {
		// `validate_chat_icon` shares the url path's screen and resolution, and
		// is reached from `create_room` and `register_bot` rather than here.
		assert_eq!(
			validate_chat_icon("icon", "https://example.invalid/\u{202e}gpj.exe"),
			Err(ChatFieldError::UnsafeCharacter { field: "icon" })
		);
		assert_eq!(
			validate_chat_icon("icon", "https://example.invalid").unwrap(),
			"https://example.invalid/"
		);
		assert_eq!(validate_chat_icon("icon", "  ").unwrap(), "");
	}

	#[test]
	fn a_name_cannot_break_its_own_line() {
		// U+2028 and U+2029 are Zl/Zp, not Cc, so `char::is_control` misses
		// them -- yet they break a line exactly like the `\n` this rejects,
		// which is what hides an extension on a one-line download affordance.
		for separator in ['\u{2028}', '\u{2029}'] {
			let spoofed = ChatMessageContent::File(ChatFile {
				url: "https://example.invalid/f".to_string(),
				file_name: format!("invoice.pdf{separator}        .exe"),
				mime_type: "application/pdf".to_string(),
				size_bytes: 1,
				text: None,
			});
			assert_eq!(
				validate_chat_message_content(spoofed),
				Err(ChatFieldError::UnsafeCharacter { field: "fileName" }),
				"{separator:?} must be rejected in a name"
			);
		}
	}

	#[test]
	fn a_url_budget_applies_to_what_the_host_receives() {
		// Resolution percent-encodes, so a URL measured on arrival can land
		// nearly three times over budget.
		// Each of these is 3 bytes raw and 9 percent-encoded, so the arriving
		// string fits the budget and the resolved one does not.
		let padded = format!("https://example.invalid/{}", "\u{4e00}".repeat(300));
		assert!(padded.len() <= CHAT_URL_MAX_BYTES);
		assert_eq!(
			validate_chat_message_content(file_with_url(&padded)),
			Err(ChatFieldError::TooLong { field: "url", limit: CHAT_URL_MAX_BYTES })
		);
	}

	#[test]
	fn action_ids_that_normalize_alike_are_rejected() {
		// Normalization maps these onto one key. Accepting both would give the
		// user two buttons whose trigger the product cannot tell apart.
		let colliding = ChatMessageContent::Actions(ChatActions {
			text: None,
			actions: vec![
				ChatAction { action_id: "approve".to_string(), title: "Approve".to_string() },
				ChatAction { action_id: " approve ".to_string(), title: "Reject".to_string() },
			],
			layout: truapi::latest::ChatActionLayout::Column,
		});
		assert_eq!(
			validate_chat_message_content(colliding),
			Err(ChatFieldError::Duplicate { field: "actionId" })
		);

		// A literal repeat is the same defect without the normalization step.
		let repeated = ChatMessageContent::Actions(ChatActions {
			text: None,
			actions: vec![
				ChatAction { action_id: "approve".to_string(), title: "Approve".to_string() },
				ChatAction { action_id: "approve".to_string(), title: "Reject".to_string() },
			],
			layout: truapi::latest::ChatActionLayout::Column,
		});
		assert_eq!(
			validate_chat_message_content(repeated),
			Err(ChatFieldError::Duplicate { field: "actionId" })
		);
	}

	#[test]
	fn a_file_name_cannot_address_a_path() {
		// A host joining this onto a cache directory must not be handed a
		// separator or a parent reference.
		for traversal in [
			"../../../../data/data/io.parity.wallet/files/session.json",
			"..",
			"a/b.txt",
			"a\\b.txt",
			"C:evil.exe",
		] {
			assert_eq!(
				validate_chat_message_content(ChatMessageContent::File(ChatFile {
					url: "https://example.invalid/f".to_string(),
					file_name: traversal.to_string(),
					mime_type: "application/pdf".to_string(),
					size_bytes: 1,
					text: None,
				})),
				Err(ChatFieldError::PathComponent { field: "fileName" }),
				"{traversal:?} must be rejected"
			);
		}
	}

	#[test]
	fn a_url_budget_is_measured_after_resolution_on_every_field() {
		// Percent-encoding grows a non-ASCII path threefold, so a value that
		// arrives inside its cap can leave well past it. Measuring the arriving
		// string alone let an icon through at three times its budget.
		let long_path = "\u{00e9}".repeat(CHAT_ICON_MAX_BYTES / 4);
		let icon = format!("https://icons.invalid/{long_path}");
		assert!(icon.len() <= CHAT_ICON_MAX_BYTES, "arrives inside its cap");
		assert!(
			Url::parse(&icon).expect("a parsable https url").to_string().len() >
				CHAT_ICON_MAX_BYTES,
			"resolves past it"
		);

		assert_eq!(
			validate_chat_icon("icon", &icon),
			Err(ChatFieldError::TooLong { field: "icon", limit: CHAT_ICON_MAX_BYTES })
		);

		let message_url =
			format!("https://files.invalid/{}", "\u{00e9}".repeat(CHAT_URL_MAX_BYTES / 4));
		assert!(message_url.len() <= CHAT_URL_MAX_BYTES);
		assert_eq!(
			validate_chat_url("url", &message_url),
			Err(ChatFieldError::TooLong { field: "url", limit: CHAT_URL_MAX_BYTES })
		);
	}

	#[test]
	fn a_url_must_not_carry_credentials() {
		// `Url::to_string` keeps `user:pass@`, so a host handed this would
		// fetch with the credentials and log them.
		for field_url in [
			"https://user:pass@example.invalid/avatar.png",
			"https://user@example.invalid/avatar.png",
		] {
			assert_eq!(
				validate_chat_icon("icon", field_url),
				Err(ChatFieldError::Credentials { field: "icon" }),
				"{field_url:?} must be rejected"
			);
			assert_eq!(
				validate_chat_url("url", field_url),
				Err(ChatFieldError::Credentials { field: "url" }),
				"{field_url:?} must be rejected"
			);
		}
	}

	#[test]
	fn reachability_is_the_hosts_decision_and_the_docs_say_so() {
		// Named rather than incidental: the trait doc tells a host these pass
		// and that fetching them is its own call. A core that guessed would
		// break a host serving its own media from localhost, so if this ever
		// starts rejecting, the doc has to change with it.
		for reachable_only_by_the_host in [
			"https://127.0.0.1:9944/rpc",
			"https://[::1]/admin",
			"https://169.254.169.254/latest/meta-data/",
			"https://10.0.0.1/internal",
		] {
			assert!(
				validate_chat_url("url", reachable_only_by_the_host).is_ok(),
				"{reachable_only_by_the_host:?} is the host's call, not the core's"
			);
		}
	}

	#[test]
	fn the_published_limits_are_the_enforced_limits() {
		// `Chat::post_message`'s doc states these numbers to products in prose,
		// so a test asserting `CONST + 1` would let the constant drift away
		// from the contract without failing.
		assert_eq!(CHAT_BODY_MAX_BYTES, 16 * 1024);
		assert_eq!(CHAT_URL_MAX_BYTES, 2048);
		assert_eq!(CHAT_ACTIONS_MAX, 32);
		assert_eq!(CHAT_MEDIA_MAX, 32);
		assert_eq!(CHAT_CUSTOM_PAYLOAD_MAX_BYTES, 256 * 1024);
		assert_eq!(CHAT_FIELD_MAX_BYTES, 256);
	}

	#[test]
	fn a_reaction_names_its_message_as_an_identifier() {
		// `message_id` addresses a message the way `room_id` addresses a room,
		// so it gets the identifier screen rather than the display one.
		let confusable = ChatMessageContent::Reaction(ChatReaction {
			message_id: "message\u{200d}-1".to_string(),
			emoji: "\u{1f3b2}".to_string(),
		});
		assert_eq!(
			validate_chat_message_content(confusable),
			Err(ChatFieldError::UnsafeCharacter { field: "messageId" })
		);

		let blank = ChatMessageContent::Reaction(ChatReaction {
			message_id: "   ".to_string(),
			emoji: "\u{1f3b2}".to_string(),
		});
		assert_eq!(
			validate_chat_message_content(blank),
			Err(ChatFieldError::Empty { field: "messageId" })
		);
	}

	#[test]
	fn auth_session_storage_key_has_stable_encoding() {
		assert_eq!(CoreStorageKey::AuthSession.encode(), [0]);
	}

	#[test]
	fn sso_responder_request_ledger_key_has_stable_encoding() {
		let key = CoreStorageKey::SsoResponderRequestLedger {
			root_public_key: [0x11; 32],
			peer_statement_account_id: [0x22; 32],
			peer_encryption_public_key: [0x33; 32],
		};
		let mut expected = vec![11];
		expected.extend([0x11; 32]);
		expected.extend([0x22; 32]);
		expected.extend([0x33; 32]);

		assert_eq!(key.encode(), expected);
	}

	#[test]
	fn product_context_encoding_matches_the_generated_host_codec() {
		// The generated TS host codec is
		// `S.Struct({productId: S.str, executionKind: S.Status("App", "Widget", "Worker")})`,
		// so the field order and the variant indices below are the wire
		// contract every JS host decodes against. The JS half of this pair is
		// `product context encoding matches the Rust platform codec` in
		// `js/packages/truapi-host/src/host-callbacks-adapter.test.ts`, which
		// asserts the same bytes through that generated codec.
		assert_eq!(ProductExecutionKind::App.encode(), [0]);
		assert_eq!(ProductExecutionKind::Widget.encode(), [1]);
		assert_eq!(ProductExecutionKind::Worker.encode(), [2]);

		let context =
			ProductContext::new_with_execution("app.dot".to_string(), ProductExecutionKind::Worker)
				.expect("product id is valid");
		assert_eq!(context.encode(), [28, b'a', b'p', b'p', b'.', b'd', b'o', b't', 2]);
		assert_eq!(ProductContext::decode(&mut context.encode().as_slice()), Ok(context));
	}

	#[test]
	fn product_context_decoding_rejects_ids_the_constructor_rejects() {
		// Hand-built frames, not values this crate can encode: `Decode` must
		// apply the same product-id policy as the constructor so decoding
		// cannot mint a context with an unscoped or empty product id.
		for product_id in ["evil.com", "", "  "] {
			let frame = (product_id.to_string(), ProductExecutionKind::App).encode();
			assert!(
				ProductContext::decode(&mut frame.as_slice()).is_err(),
				"{product_id:?} must not decode into a ProductContext"
			);
		}
	}

	#[test]
	fn product_context_decoding_normalizes_the_product_id() {
		// Derivation and product-scoped storage are keyed by `product_id`, so
		// a non-canonical id off the wire has to land in the same scope the
		// constructor would produce rather than opening a second one.
		let frame = ("App.DOT".to_string(), ProductExecutionKind::App).encode();
		let decoded = ProductContext::decode(&mut frame.as_slice()).expect("product id normalizes");
		assert_eq!(decoded.product_id, "app.dot");
		assert_eq!(
			decoded,
			ProductContext::new("app.dot".to_string()).expect("product id is valid")
		);
	}

	#[test]
	fn trusted_remote_permission_labels_match_the_bare_product_label() {
		for product_id in [
			"peopl.dot",
			"peopl.paseo",
			"peopl.testnet",
			"dim2.dot",
			"dim2.paseo",
			"dim2.testnet",
			"stash.dot",
		] {
			assert!(
				has_trusted_remote_permissions(product_id),
				"{product_id} must hold remote permissions without a prompt"
			);
		}
		for product_id in [
			"app.peopl.dot",
			"sub.dim2.paseo",
			"app.dim2.testnet",
			"dim2.com",
			"notdim2.dot",
			"peopl",
			"peopl.com",
			"peoplx.dot",
			"my-peopl.dot",
			"localhost",
			"localhost:3000",
			"",
			"dot",
		] {
			assert!(
				!has_trusted_remote_permissions(product_id),
				"{product_id} is a separate product and must prompt"
			);
		}
	}

	#[test]
	fn a_published_product_cannot_take_the_localhost_label() {
		// Hosts grant a development wildcard to a localhost product, so the
		// label must not be reachable from dotNS.
		for product_id in ["localhost.dot", "localhost.paseo", "LOCALHOST.DOT"] {
			assert!(
				normalize_product_identifier(product_id).is_err(),
				"{product_id} must not be a valid product identifier"
			);
			assert!(
				!is_localhost_product_identifier(product_id),
				"{product_id} must not read as the development wildcard"
			);
		}
		for product_id in ["localhost", "localhost:3000"] {
			assert!(is_localhost_product_identifier(product_id), "{product_id}");
		}
		// A prefix test would admit these; the whole identifier has to match.
		for product_id in ["localhost:3000.evil.dot", "localhost:", "localhost:80a"] {
			assert!(
				!is_localhost_product_identifier(product_id),
				"{product_id} must not read as the development wildcard"
			);
		}
		// Still a normal product, sharing only the prefix.
		assert!(normalize_product_identifier("localhosting.dot").is_ok());
	}

	#[test]
	fn trusted_remote_permissions_normalize_the_spellings_a_host_holds() {
		// Both the UniFFI and wasm exports answer through this, and a host
		// holds an id in whatever spelling it received rather than the
		// normalized form the core passes internally.
		for product_id in
			["peopl.dot", "PEOPL.DOT", "  peopl.dot  ", "dim2.paseo", " DIM2.TESTNET ", "stash.dot"]
		{
			assert!(
				normalizes_to_trusted_remote_permissions(product_id),
				"{product_id} is a first-party product in any accepted spelling"
			);
		}
		for product_id in ["app.peopl.dot", "peopl", "notpeopl.dot", "localhost:3000", "", "   "] {
			assert!(
				!normalizes_to_trusted_remote_permissions(product_id),
				"{product_id} must not be read as trusted"
			);
		}
	}

	#[test]
	fn every_trusted_remote_permission_label_is_a_product_identifier() {
		// A label that product-id validation rejects would never reach the
		// permission engine, so the whitelist entry would be silently inert.
		for label in REMOTE_PERMISSION_TRUSTED_LABELS {
			for tld in DOTNS_TLDS {
				let product_id = format!("{label}.{tld}");
				assert!(
					is_product_identifier(&product_id),
					"{product_id} must be an accepted product identifier"
				);
				assert!(
					has_trusted_remote_permissions(&product_id),
					"{product_id} must be recognized as trusted"
				);
			}
		}
	}

	#[test]
	fn trusted_remote_permission_labels_are_bare_lowercase_labels() {
		// The predicate compares against the label of an already-normalized id,
		// so an entry carrying a TLD or an uppercase letter can never match.
		for label in REMOTE_PERMISSION_TRUSTED_LABELS {
			assert!(!label.contains('.'), "{label} must not carry a TLD");
			assert_eq!(*label, label.to_lowercase(), "{label} must be lowercase");
		}
	}

	#[test]
	fn an_identifier_carrying_no_name_is_rejected() {
		// Each of these reached the manifest grant key, the cache key, the only
		// consent prompt in this design and every log line, because only the
		// suffix after the last dot was ever inspected.
		for id in [
			"dim2\u{0}.dot",        // NUL, into a prompt rendered verbatim
			"\u{202e}dim2.dot",     // right-to-left override
			"dim2\u{200b}.dot",     // zero width space
			"a b c.dot",            // whitespace
			"../../etc/passwd.dot", // path separators
			"dim2..dot",            // empty label: collapses onto other ids
			".dot",                 // same empty label
		] {
			assert!(
				normalize_product_identifier(id).is_err(),
				"{id:?} carries no product name and must not normalize"
			);
		}
	}

	#[test]
	fn an_internationalized_identifier_is_still_an_identifier() {
		// The rejection above is a hazard list, not an ASCII allowlist: dotNS
		// names are internationalized and these are real ids.
		for id in ["Tést.DOT", "münchen.dot", "dim2-two.dot", "localhost:3000"] {
			assert!(normalize_product_identifier(id).is_ok(), "{id:?} is a legitimate product id");
		}
	}

	#[test]
	fn an_overlong_product_id_is_not_an_identifier() {
		// Only the suffix after the last `.` is checked, so every length of
		// this is otherwise a valid, distinct, wire-supplied cache key.
		let label = "a".repeat(PRODUCT_ID_MAX_BYTES);
		let overlong = format!("{label}.dot");
		assert!(overlong.len() > PRODUCT_ID_MAX_BYTES);
		assert!(
			!is_product_identifier(&overlong),
			"a product id past the cap must be rejected, not stored"
		);

		// A huge id alone would pass with the cap off by any amount.
		let at_cap = format!("{}.dot", "a".repeat(PRODUCT_ID_MAX_BYTES - 4));
		assert_eq!(at_cap.len(), PRODUCT_ID_MAX_BYTES);
		assert!(is_product_identifier(&at_cap), "an id exactly at the cap is still valid");
	}

	#[test]
	fn core_storage_key_description_is_strict_and_product_scoped() {
		let permission = CoreStorageKey::device_permission_authorization(
			"product.dot",
			&HostDevicePermissionRequest::Camera,
		)
		.encode();
		assert_eq!(
			describe_core_storage_key(&permission),
			Ok(CoreStorageKeyDescription {
				kind: "PermissionAuthorization",
				product_id: Some("product.dot".to_string()),
			})
		);
		for (key, kind, product_id) in [
			(CoreStorageKey::AuthSession, "AuthSession", None),
			(CoreStorageKey::PairingDeviceIdentity, "PairingDeviceIdentity", None),
			(
				CoreStorageKey::AllowanceKeys { session_id: "session".to_string() },
				"AllowanceKeys",
				None,
			),
			(CoreStorageKey::LastProcessedPairingStatement, "LastProcessedPairingStatement", None),
			(
				CoreStorageKey::AutoSigningKey { product_id: "product.dot".to_string() },
				"AutoSigningKey",
				Some("product.dot"),
			),
			(CoreStorageKey::AutoSigningKeys, "AutoSigningKeys", None),
			(
				CoreStorageKey::RingVrfRegistry { root_public_key: [0x42; 32] },
				"RingVrfRegistry",
				None,
			),
			(CoreStorageKey::StatementRenewalTargets, "StatementRenewalTargets", None),
			(CoreStorageKey::DeviceEncryptionKey, "DeviceEncryptionKey", None),
			(CoreStorageKey::ChatDeviceKey, "ChatDeviceKey", None),
			(
				CoreStorageKey::SsoResponderRequestLedger {
					root_public_key: [0x11; 32],
					peer_statement_account_id: [0x22; 32],
					peer_encryption_public_key: [0x33; 32],
				},
				"SsoResponderRequestLedger",
				None,
			),
			(CoreStorageKey::FundingSessions, "FundingSessions", None),
			(CoreStorageKey::FundingSessionsUnreadable, "FundingSessionsUnreadable", None),
			(CoreStorageKey::FundingSessionPack, "FundingSessionPack", None),
		] {
			let description = describe_core_storage_key(&key.encode()).expect("valid key");
			assert_eq!(description.kind, kind);
			assert_eq!(description.product_id.as_deref(), product_id);
		}

		assert_eq!(
			describe_core_storage_key(&[]),
			Err(CoreStorageKeyDescriptionError::InvalidEncoding)
		);
		let mut trailing = CoreStorageKey::AuthSession.encode();
		trailing.push(0);
		assert_eq!(
			describe_core_storage_key(&trailing),
			Err(CoreStorageKeyDescriptionError::TrailingBytes)
		);
		assert_eq!(
			describe_core_storage_key(&[u8::MAX]),
			Err(CoreStorageKeyDescriptionError::InvalidEncoding)
		);
	}

	#[test]
	fn permission_authorization_keys_separate_product_and_request_variants() {
		let camera = CoreStorageKey::device_permission_authorization(
			"product.dot",
			&HostDevicePermissionRequest::Camera,
		);
		let other_product = CoreStorageKey::device_permission_authorization(
			"other.dot",
			&HostDevicePermissionRequest::Camera,
		);
		let remote = CoreStorageKey::remote_permission_authorization(
			"product.dot",
			&RemotePermissionRequest { permission: RemotePermission::ChainSubmit },
		);
		let identity = CoreStorageKey::identity_disclosure_authorization("product.dot");
		let other_product_identity = CoreStorageKey::identity_disclosure_authorization("other.dot");
		let account_access =
			CoreStorageKey::account_access_authorization("product.dot", "target.dot");
		let other_target = CoreStorageKey::account_access_authorization("product.dot", "other.dot");

		assert_ne!(camera, other_product);
		assert_ne!(camera, remote);
		assert_ne!(camera, identity);
		assert_ne!(remote, identity);
		assert_ne!(identity, other_product_identity);
		assert_ne!(account_access, other_target);
		assert_ne!(account_access, camera);
	}

	#[test]
	fn remote_domain_candidates_match_legacy_wildcard_coverage() {
		assert_eq!(
			remote_domain_candidates("api.example.com"),
			["api.example.com", "*.example.com", "*"]
		);
		assert_eq!(
			remote_domain_candidates("deep.api.example.com"),
			["deep.api.example.com", "*.api.example.com", "*.example.com", "*"]
		);
		assert_eq!(remote_domain_candidates("example.com"), ["example.com", "*"]);
		assert_eq!(remote_domain_candidates("wallet.dot"), ["wallet.dot", "*"]);
		// A single-label host has no parent to wildcard over.
		assert_eq!(remote_domain_candidates("localhost"), ["localhost", "*"]);
		// A stored pattern resolves to itself, not to a duplicated entry.
		assert_eq!(remote_domain_candidates("*.example.com"), ["*.example.com", "*"]);
		assert_eq!(remote_domain_candidates("*"), ["*"]);
		assert_eq!(remote_domain_candidates("*.com"), ["*.com", "*"]);
		assert_eq!(
			remote_domain_candidates("*.api.example.com"),
			["*.api.example.com", "*.example.com", "*"]
		);
		assert_eq!(remote_domain_candidates("example.co.uk"), ["example.co.uk", "*.co.uk", "*"]);
		assert_eq!(remote_domain_candidates("127.0.0.1"), ["127.0.0.1", "*"]);
		assert_eq!(remote_domain_candidates("[::1]"), ["[::1]", "*"]);
		assert_eq!(
			remote_domain_candidates("DEEP.API.Bücher.Example."),
			[
				"deep.api.xn--bcher-kva.example",
				"*.api.xn--bcher-kva.example",
				"*.xn--bcher-kva.example",
				"*",
			]
		);
		for (pattern, valid) in [
			("example.com", true),
			("localhost", true),
			("127.0.0.1", true),
			("[::1]", true),
			("*", true),
			("*.example.com", true),
			("*.co.uk", true),
			("*.Bücher.example.", true),
			("*.com", false),
			("*.dot", false),
			("*.127.0.0.1", false),
			("*.0.1", false),
			("*.[::1]", false),
			("*..com", false),
			("api.*.com", false),
			("", false),
		] {
			assert_eq!(is_valid_remote_domain_pattern(pattern), valid, "{pattern}");
		}
	}

	#[test]
	fn remote_domain_normalization_is_shared_by_both_ends_of_a_lookup() {
		// The forms enforcement can hand in from a real URL host all collapse
		// onto the one key a grant is stored under.
		for spelling in ["API.Example.COM", "api.example.com.", "  api.example.com  "] {
			assert_eq!(normalize_remote_domain(spelling), "api.example.com");
		}
		// IDNA: a non-ASCII host and its punycode spelling are one site, so
		// they must be one slot and one prompt.
		assert_eq!(normalize_remote_domain("Bücher.example"), "xn--bcher-kva.example");
		assert_eq!(normalize_remote_domain("xn--bcher-kva.example"), "xn--bcher-kva.example");
		// The wildcard marker is not part of the host and survives untouched.
		assert_eq!(normalize_remote_domain("*.Example.COM"), "*.example.com");
		assert_eq!(normalize_remote_domain("*.Bücher.example"), "*.xn--bcher-kva.example");
		assert_eq!(normalize_remote_domain("*"), "*");
		// A canonicalized bundle keys the same as the candidate list built from
		// a live host, which is what makes the grant visible to enforcement.
		assert_eq!(
			CoreStorageKey::remote_domain_authorization("product.dot", "API.Example.COM."),
			CoreStorageKey::remote_domain_authorization(
				"product.dot",
				&remote_domain_candidates("api.example.com.")[0]
			)
		);
	}

	#[test]
	fn remote_domain_authorization_key_matches_the_one_element_bundle() {
		// Enforcement keys a single host; a product granting that one domain
		// must land in the same slot, or the grant is invisible to the gate.
		assert_eq!(
			CoreStorageKey::remote_domain_authorization("product.dot", "Example.COM"),
			CoreStorageKey::remote_permission_authorization(
				"product.dot",
				&RemotePermissionRequest {
					permission: RemotePermission::Remote {
						domains: vec!["example.com".to_string()],
					},
				}
			)
		);
		assert_ne!(
			CoreStorageKey::remote_domain_authorization("product.dot", "example.com"),
			CoreStorageKey::remote_domain_authorization("product.dot", "*.example.com")
		);
		assert_ne!(
			CoreStorageKey::remote_domain_authorization("product.dot", "example.com"),
			CoreStorageKey::remote_domain_authorization("other.dot", "example.com")
		);
	}

	#[test]
	fn remote_permission_authorization_key_canonicalizes_domain_sets() {
		let unsorted = RemotePermissionRequest {
			permission: RemotePermission::Remote {
				domains: vec!["b.example.com".into(), "a.example.com".into()],
			},
		};
		let sorted = RemotePermissionRequest {
			permission: RemotePermission::Remote {
				domains: vec!["a.example.com".into(), "b.example.com".into()],
			},
		};
		assert_eq!(
			CoreStorageKey::remote_permission_authorization("product.dot", &unsorted),
			CoreStorageKey::remote_permission_authorization("product.dot", &sorted)
		);

		let mixed = RemotePermissionRequest {
			permission: RemotePermission::Remote {
				domains: vec!["Example.COM".into(), "a.com".into(), "a.com".into()],
			},
		};
		let canonical = RemotePermissionRequest {
			permission: RemotePermission::Remote {
				domains: vec!["a.com".into(), "example.com".into()],
			},
		};
		assert_eq!(
			CoreStorageKey::remote_permission_authorization("product.dot", &mixed),
			CoreStorageKey::remote_permission_authorization("product.dot", &canonical)
		);
	}

	#[test]
	fn remote_permission_authorization_key_handles_separator_chars_in_domains() {
		let injecting = RemotePermissionRequest {
			permission: RemotePermission::Remote {
				domains: vec!["a|b".into(), "c,d".into(), "remote:web-rtc".into()],
			},
		};
		let benign_same_set = RemotePermissionRequest {
			permission: RemotePermission::Remote {
				domains: vec!["x".into(), "y".into(), "z".into()],
			},
		};
		let injecting_key =
			CoreStorageKey::remote_permission_authorization("product.dot", &injecting);
		let benign_key =
			CoreStorageKey::remote_permission_authorization("product.dot", &benign_same_set);
		assert_ne!(injecting_key, benign_key);

		let webrtc = RemotePermissionRequest { permission: RemotePermission::WebRtc };
		assert_ne!(
			injecting_key,
			CoreStorageKey::remote_permission_authorization("product.dot", &webrtc)
		);

		let injecting_reordered = RemotePermissionRequest {
			permission: RemotePermission::Remote {
				domains: vec!["remote:web-rtc".into(), "c,d".into(), "a|b".into()],
			},
		};
		assert_eq!(
			injecting_key,
			CoreStorageKey::remote_permission_authorization("product.dot", &injecting_reordered)
		);
	}

	/// A host decodes reviews with its own codec. If a discriminant moves, a
	/// host one version apart shows the user a different consent prompt from
	/// the one its answer approves.
	#[test]
	fn review_discriminants_do_not_move() {
		let discriminants = [
			UserConfirmationReview::AccountAccess(AccountAccessReview {
				requesting_product_id: "caller.dot".to_string(),
				target_product_id: "target.dot".to_string(),
			}),
			UserConfirmationReview::ProductSubtree(ProductSubtreeReview {
				product_id: "caller.dot".to_string(),
			}),
		]
		.map(|review| review.encode()[0]);

		assert_eq!(discriminants, [9, 11]);
	}

	fn text_message(message_id: &str) -> ChatMessage {
		ChatMessage {
			message_id: message_id.to_string(),
			origin: ChatMessageOrigin { participant_id: "product".to_string() },
			timestamp: 1,
			body: ChatMessageBody::Text { text: "hi".to_string() },
			status: truapi::latest::ChatMessageStatus::Sent,
			reply_to: None,
			edited_at: None,
			reactions: Vec::new(),
		}
	}

	fn png(len: usize) -> ChatResource {
		ChatResource::Inline { mime_type: "image/png".to_string(), bytes: vec![1; len] }
	}

	fn image_item(resource: ChatResource) -> ChatMediaItem {
		ChatMediaItem::Image {
			resource,
			meta: ChatMediaMeta {
				mime_type: "image/png".to_string(),
				size_bytes: None,
				width: None,
				height: None,
				placeholder: None,
				alt: None,
			},
		}
	}

	fn callback_action(action_id: String) -> ChatProductAction {
		ChatProductAction {
			action_id,
			label: "Pin".to_string(),
			icon: None,
			destructive: false,
			handler: ChatActionHandler::Callback,
		}
	}

	fn hop(identifier: usize, node: &str, size: u64) -> ChatResource {
		ChatResource::Hop {
			identifier: vec![1; identifier],
			ticket: vec![2; 32],
			node: node.to_string(),
			size,
		}
	}

	// A HOP file is media only: its ticket would leak through an avatar, and a
	// node that is not `wss://` is not one the apps dial.
	#[test]
	fn a_hop_file_is_media_with_a_wss_node() {
		let file = hop(32, "wss://hop.example", 1_000);
		assert_eq!(validate_chat_media_resource("media", file.clone()), Ok(file.clone()));
		assert_eq!(
			validate_chat_image_resource("avatar", file),
			Err(ChatFieldError::RejectedScheme { field: "avatar" })
		);
		assert_eq!(
			validate_chat_media_resource("media", hop(31, "wss://hop.example", 1_000)),
			Err(ChatFieldError::WrongLength { field: "media", length: 32 })
		);
		assert_eq!(
			validate_chat_media_resource("media", hop(32, "https://hop.example", 1_000)),
			Err(ChatFieldError::RejectedScheme { field: "media" })
		);
		assert!(
			validate_chat_media_resource(
				"media",
				hop(32, "wss://hop.example", CHAT_HOP_FILE_MAX_BYTES + 1)
			)
			.is_err()
		);
	}

	// The host draws an avatar or an icon in its own chrome, so only a raster
	// image within the icon budget reaches it; a preimage key is the hash the
	// Preimage API addresses, nothing else.
	#[test]
	fn an_image_resource_is_a_small_raster_or_a_preimage_key() {
		assert_eq!(
			validate_chat_image_resource("avatar", png(CHAT_ICON_MAX_BYTES)),
			Ok(png(CHAT_ICON_MAX_BYTES))
		);
		assert_eq!(
			validate_chat_image_resource("avatar", png(CHAT_ICON_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong { field: "avatar", limit: CHAT_ICON_MAX_BYTES })
		);
		assert_eq!(
			validate_chat_image_resource(
				"avatar",
				ChatResource::Inline { mime_type: "image/svg+xml".to_string(), bytes: vec![1] }
			),
			Err(ChatFieldError::RejectedScheme { field: "avatar" })
		);
		assert_eq!(
			validate_chat_image_resource(
				"avatar",
				ChatResource::Inline { mime_type: " Image/PNG ".to_string(), bytes: vec![1] }
			),
			Ok(png(1))
		);
		assert_eq!(
			validate_chat_image_resource("avatar", png(0)),
			Err(ChatFieldError::Empty { field: "avatar" })
		);
		let key = ChatResource::Preimage { key: vec![7; CHAT_PREIMAGE_KEY_BYTES] };
		assert_eq!(validate_chat_image_resource("avatar", key.clone()), Ok(key));
		assert_eq!(
			validate_chat_image_resource("avatar", ChatResource::Preimage { key: vec![7; 31] }),
			Err(ChatFieldError::WrongLength { field: "avatar", length: CHAT_PREIMAGE_KEY_BYTES })
		);
	}

	#[test]
	fn a_media_item_is_bounded_by_the_media_budget_not_the_icon_budget() {
		let body =
			|len| ChatMessageBody::RichText { text: None, media: vec![image_item(png(len))] };
		assert!(validate_chat_message_body(body(CHAT_INLINE_MEDIA_MAX_BYTES)).is_ok());
		assert_eq!(
			validate_chat_message_body(body(CHAT_INLINE_MEDIA_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong { field: "media", limit: CHAT_INLINE_MEDIA_MAX_BYTES })
		);
		let many = |count| ChatMessageBody::RichText {
			text: None,
			media: (0..count).map(|_| image_item(png(1))).collect(),
		};
		assert!(validate_chat_message_body(many(CHAT_MEDIA_MAX)).is_ok());
		assert_eq!(
			validate_chat_message_body(many(CHAT_MEDIA_MAX + 1)),
			Err(ChatFieldError::TooMany { field: "media", limit: CHAT_MEDIA_MAX })
		);
	}

	// `text` and `payload` are the only fields a product shrinks by sending
	// less, so their size rejections keep those names for `MessageTooLarge`.
	#[test]
	fn a_message_body_is_bounded_in_text_and_payload() {
		let text = |len| ChatMessageBody::Text { text: "x".repeat(len) };
		assert!(validate_chat_message_body(text(CHAT_BODY_MAX_BYTES)).is_ok());
		assert_eq!(
			validate_chat_message_body(text(CHAT_BODY_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong { field: "text", limit: CHAT_BODY_MAX_BYTES })
		);
		let custom = |len| ChatMessageBody::Custom {
			message_type: "vote".to_string(),
			payload: vec![0; len],
			preview_text: None,
		};
		assert!(validate_chat_message_body(custom(CHAT_CUSTOM_PAYLOAD_MAX_BYTES)).is_ok());
		assert_eq!(
			validate_chat_message_body(custom(CHAT_CUSTOM_PAYLOAD_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong { field: "payload", limit: CHAT_CUSTOM_PAYLOAD_MAX_BYTES })
		);
		let preview = ChatMessageBody::Custom {
			message_type: "vote".to_string(),
			payload: Vec::new(),
			preview_text: Some("x".repeat(CHAT_BODY_MAX_BYTES + 1)),
		};
		assert_eq!(
			validate_chat_message_body(preview),
			Err(ChatFieldError::TooLong { field: "previewText", limit: CHAT_BODY_MAX_BYTES })
		);
	}

	#[test]
	fn a_message_is_screened_in_every_identifier_and_bounded_in_reactions() {
		assert_eq!(
			validate_chat_message(text_message(" m1 ")).map(|message| message.message_id),
			Ok("m1".to_string())
		);
		assert_eq!(
			validate_chat_message(text_message("m\u{200d}1")),
			Err(ChatFieldError::UnsafeCharacter { field: "messageId" })
		);
		let reacted = |count: usize| ChatMessage {
			reactions: (0..count)
				.map(|index| ChatReactionSummary {
					emoji: format!("e{index}"),
					count: 1,
					reacted_by_user: false,
				})
				.collect(),
			..text_message("m1")
		};
		assert!(validate_chat_message(reacted(CHAT_REACTIONS_MAX)).is_ok());
		assert_eq!(
			validate_chat_message(reacted(CHAT_REACTIONS_MAX + 1)),
			Err(ChatFieldError::TooMany { field: "reactions", limit: CHAT_REACTIONS_MAX })
		);
		assert_eq!(
			validate_chat_feed_update(ChatFeedUpdate::ReactionPlaced {
				message_id: "m1".to_string(),
				participant_id: "user\u{202e}".to_string(),
				emoji: "👍".to_string(),
			}),
			Err(ChatFieldError::UnsafeCharacter { field: "participantId" })
		);
	}

	#[test]
	fn room_declarations_are_bounded_as_the_limits_say() {
		let metadata = |description: usize| ChatRoomMetadata {
			name: "Support".to_string(),
			avatar: None,
			description: Some("x".repeat(description)),
		};
		assert!(validate_chat_room_metadata(metadata(CHAT_DESCRIPTION_MAX_BYTES)).is_ok());
		assert_eq!(
			validate_chat_room_metadata(metadata(CHAT_DESCRIPTION_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong {
				field: "description",
				limit: CHAT_DESCRIPTION_MAX_BYTES
			})
		);
		assert_eq!(
			validate_chat_room_metadata(ChatRoomMetadata {
				name: "x".repeat(CHAT_FIELD_MAX_BYTES + 1),
				..metadata(0)
			}),
			Err(ChatFieldError::TooLong { field: "name", limit: CHAT_FIELD_MAX_BYTES })
		);

		// The room placeholder is content shown in an empty room, so it takes
		// the body budget; the input hint is one line and takes the name budget.
		let config = |placeholder: usize| ChatRoomConfig {
			placeholder: Some("x".repeat(placeholder)),
			..ChatRoomConfig::default()
		};
		assert!(validate_chat_room_config(config(CHAT_BODY_MAX_BYTES)).is_ok());
		assert_eq!(
			validate_chat_room_config(config(CHAT_BODY_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong { field: "placeholder", limit: CHAT_BODY_MAX_BYTES })
		);
		let hint = |len: usize| {
			ChatRoomFooter::Input(ChatInputConfig {
				placeholder: Some("x".repeat(len)),
				..ChatInputConfig::default()
			})
		};
		assert!(validate_chat_room_footer(hint(CHAT_FIELD_MAX_BYTES)).is_ok());
		assert_eq!(
			validate_chat_room_footer(hint(CHAT_FIELD_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong { field: "placeholder", limit: CHAT_FIELD_MAX_BYTES })
		);

		let reactions = |emoji: Vec<String>| ChatRoomConfig {
			allowed_reactions: Some(emoji),
			..ChatRoomConfig::default()
		};
		assert_eq!(
			validate_chat_room_config(reactions(
				(0..=CHAT_REACTIONS_MAX).map(|index| format!("e{index}")).collect()
			)),
			Err(ChatFieldError::TooMany { field: "allowedReactions", limit: CHAT_REACTIONS_MAX })
		);
		assert_eq!(
			validate_chat_room_config(reactions(vec!["👍".to_string(), " 👍".to_string()])),
			Err(ChatFieldError::Duplicate { field: "allowedReactions" })
		);

		let preview = ChatRoomPreview {
			order: truapi::latest::ChatRoomOrder::PinToTop,
			timestamp: None,
			badge: Some(ChatRoomBadge::Image { resource: png(CHAT_ICON_MAX_BYTES + 1) }),
			text: None,
		};
		assert_eq!(
			validate_chat_room_preview(preview),
			Err(ChatFieldError::TooLong { field: "badge", limit: CHAT_ICON_MAX_BYTES })
		);
	}

	#[test]
	fn room_lists_are_bounded_by_count_and_unique_by_id() {
		let menu = |count: usize| ChatMessageMenu::Custom {
			actions: (0..count).map(|index| callback_action(format!("a{index}"))).collect(),
		};
		assert!(validate_chat_message_menu(menu(CHAT_MENU_ACTIONS_MAX)).is_ok());
		assert_eq!(
			validate_chat_message_menu(menu(CHAT_MENU_ACTIONS_MAX + 1)),
			Err(ChatFieldError::TooMany { field: "actions", limit: CHAT_MENU_ACTIONS_MAX })
		);
		assert_eq!(
			validate_chat_message_menu(ChatMessageMenu::ExtendBuiltIn {
				actions: vec![callback_action("pin".into()), callback_action(" pin".into())],
				hidden: Vec::new(),
			}),
			Err(ChatFieldError::Duplicate { field: "actionId" })
		);
	}

	// A deeplink is opened by the host, so it passes the same policy as
	// `system.navigate_to` and arrives in the form that call would hand over.
	#[test]
	fn a_deeplink_passes_the_navigation_policy() {
		let menu = |url: String| ChatMessageMenu::Custom {
			actions: vec![ChatProductAction {
				handler: ChatActionHandler::Deeplink { url },
				..callback_action("open".into())
			}],
		};
		assert_eq!(
			validate_chat_message_menu(menu("javascript:alert(1)".into())),
			Err(ChatFieldError::RejectedScheme { field: "url" })
		);
		assert_eq!(
			validate_chat_message_menu(menu(format!(
				"https://a.example/{}",
				"x".repeat(CHAT_CURSOR_MAX_BYTES)
			))),
			Err(ChatFieldError::TooLong { field: "url", limit: CHAT_CURSOR_MAX_BYTES })
		);
		let Ok(ChatMessageMenu::Custom { actions }) =
			validate_chat_message_menu(menu("mytestapp.dot/settings".into()))
		else {
			panic!("a dotNS deeplink is accepted");
		};
		assert_eq!(
			actions[0].handler,
			ChatActionHandler::Deeplink { url: "polkadot://mytestapp.dot/settings".to_string() }
		);
	}

	#[test]
	fn a_query_asks_a_bounded_page() {
		let messages = |page_size, cursor: usize| ProductChatMessagesQueryRequest {
			room_id: "room".to_string(),
			anchor: ChatPageAnchor::Before { cursor: Some("c".repeat(cursor)) },
			page_size,
		};
		assert!(
			validate_chat_messages_query(messages(CHAT_MESSAGES_PAGE_MAX, CHAT_CURSOR_MAX_BYTES))
				.is_ok()
		);
		assert!(validate_chat_messages_query(messages(CHAT_MESSAGES_PAGE_DEFAULT, 1)).is_ok());
		for page_size in [0, CHAT_MESSAGES_PAGE_MAX + 1] {
			assert_eq!(
				validate_chat_messages_query(messages(page_size, 1)),
				Err(ChatFieldError::OutOfRange {
					field: "pageSize",
					min: 1,
					max: CHAT_MESSAGES_PAGE_MAX.into()
				})
			);
		}
		assert_eq!(
			validate_chat_messages_query(messages(1, CHAT_CURSOR_MAX_BYTES + 1)),
			Err(ChatFieldError::TooLong { field: "cursor", limit: CHAT_CURSOR_MAX_BYTES })
		);

		let participants =
			|query| ProductChatParticipantsQueryRequest { room_id: "room".to_string(), query };
		let page = |page_size| ChatParticipantsQuery::Page { cursor: None, page_size };
		assert!(
			validate_chat_participants_query(participants(page(CHAT_PARTICIPANTS_PAGE_MAX)))
				.is_ok()
		);
		assert!(
			validate_chat_participants_query(participants(page(CHAT_PARTICIPANTS_PAGE_DEFAULT)))
				.is_ok()
		);
		assert_eq!(
			validate_chat_participants_query(participants(page(CHAT_PARTICIPANTS_PAGE_MAX + 1))),
			Err(ChatFieldError::OutOfRange {
				field: "pageSize",
				min: 1,
				max: CHAT_PARTICIPANTS_PAGE_MAX.into()
			})
		);
		let by_ids = |count: usize| ChatParticipantsQuery::ByIds {
			participant_ids: (0..count).map(|index| format!("p{index}")).collect(),
		};
		assert!(
			validate_chat_participants_query(participants(by_ids(CHAT_PARTICIPANT_IDS_MAX)))
				.is_ok()
		);
		assert_eq!(
			validate_chat_participants_query(participants(by_ids(CHAT_PARTICIPANT_IDS_MAX + 1))),
			Err(ChatFieldError::TooMany {
				field: "participantIds",
				limit: CHAT_PARTICIPANT_IDS_MAX
			})
		);
	}

	// A product answers no more than the host asked for, so a page cannot grow
	// the cache past what the screen requested.
	#[test]
	fn an_answer_holds_no_more_than_the_page_asked() {
		let page = |count: usize| ProductChatMessagesQueryItem {
			messages: (0..count).map(|index| text_message(&format!("m{index}"))).collect(),
			next: None,
		};
		assert!(validate_chat_messages_page(page(3), 3).is_ok());
		assert_eq!(
			validate_chat_messages_page(page(4), 3),
			Err(ChatFieldError::TooMany { field: "messages", limit: 3 })
		);
		assert_eq!(
			validate_chat_participants_answer(ProductChatParticipantsQueryItem::Resolved {
				participants: vec![ChatParticipantEntry {
					participant_id: "guide".to_string(),
					participant: ChatParticipant::Custom {
						nickname: "Guide\u{202e}".to_string(),
						avatar: None,
					},
				}],
			}),
			Err(ChatFieldError::UnsafeCharacter { field: "nickname" })
		);
	}

	// S44: the 21st feed update in one second is refused, and the budget comes
	// back at the rate it is spent.
	#[test]
	fn the_rate_limiter_holds_each_room_to_its_budget() {
		let limiter = ChatRateLimiter::new(CHAT_FEED_UPDATES_PER_SECOND);
		let start = 1_000_000;
		for _ in 0..CHAT_FEED_UPDATES_PER_SECOND {
			assert!(limiter.try_acquire("chat.dot", "room", start));
		}
		assert!(!limiter.try_acquire("chat.dot", "room", start));
		// Another room and another product keep their own budgets.
		assert!(limiter.try_acquire("chat.dot", "other", start));
		assert!(limiter.try_acquire("other.dot", "room", start));
		// At 20 per second one token comes back every 50 ms.
		assert!(!limiter.try_acquire("chat.dot", "room", start + 49));
		assert!(limiter.try_acquire("chat.dot", "room", start + 50));
		assert!(!limiter.try_acquire("chat.dot", "room", start + 50));

		let previews = ChatRateLimiter::new(CHAT_PREVIEW_UPDATES_PER_SECOND);
		for _ in 0..CHAT_PREVIEW_UPDATES_PER_SECOND {
			assert!(previews.try_acquire("chat.dot", "room", start));
		}
		assert!(!previews.try_acquire("chat.dot", "room", start));
		previews.forget("chat.dot", "room");
		assert!(previews.try_acquire("chat.dot", "room", start));
	}
}

/// Host-private persistence for core-owned state.
///
/// Clearing product-indexed slots is the host's job. The core drops the ones
/// it is holding when a session ends, but a product it never opened this run
/// has no entry to drop, so those slots outlive the disconnect. A host that
/// removes a product must clear them with the rest of that product's state, or
/// they accumulate for the life of the install.
///
/// [`describe_core_storage_key`] names the product owning a slot:
/// [`CoreStorageKeyDescription::product_id`] is `Some` exactly for the
/// product-indexed variants, which are `PermissionAuthorization`,
/// `AutoSigningKey`, and `ProductSubtree`. Keying host storage by that value
/// makes the sweep a prefix delete rather than a scan.
// TODO: introduce SecretStorage to make protection a separate contract.
#[async_trait]
pub trait CoreStorage: Send + Sync {
	/// Read a core-owned value by typed slot.
	async fn read_core_storage(&self, key: CoreStorageKey)
	-> Result<Option<Vec<u8>>, GenericError>;

	/// Write a core-owned value by typed slot.
	async fn write_core_storage(
		&self,
		key: CoreStorageKey,
		value: Vec<u8>,
	) -> Result<(), GenericError>;

	/// Clear a core-owned value by typed slot.
	async fn clear_core_storage(&self, key: CoreStorageKey) -> Result<(), GenericError>;
}

/// Decoded session fields a host shell needs to render account UI without
/// parsing the opaque session blob the core persists through [`CoreStorage`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct SessionUiInfo {
	/// 32-byte sr25519 root public key of the active session.
	pub public_key: Bytes32,
	/// Wallet identity account id used for the dotNS username lookup on Asset Hub.
	pub identity_account_id: Option<Bytes32>,
	/// X25519 public key addressing this identity in chat. Public counterpart
	/// of the key [`CoreAdmin::get_session_chat_identity_key`] serves.
	pub chat_public_key: Option<Bytes32>,
	/// X25519 public key of the wallet device that answered pairing. Hosts
	/// running their own encrypted device-sync channel key it against this.
	pub device_enc_public_key: Option<Bytes32>,
	/// Statement-store account id the paired wallet signs every session-channel
	/// statement with. Whether it is scoped to the wallet device or to the
	/// wallet identity is the wallet's choice, so hosts must not treat it as a
	/// device discriminator; use [`Self::device_enc_public_key`] for that.
	pub peer_statement_account_id: Option<Bytes32>,
	/// Statement-store account id this device advertises in the pairing
	/// proposal; the paired wallet registers the statement-store allowance for
	/// it, and peers address this host on topics derived from it. Rotated per
	/// login. Secret at [`CoreAdmin::get_device_statement_key`].
	pub device_statement_account_id: Option<Bytes32>,
	/// Short username from the dotNS identity record on Asset Hub.
	pub lite_username: Option<String>,
	/// Fully qualified username from the dotNS identity record on Asset Hub.
	pub full_username: Option<String>,
}

/// Auth/session lifecycle state the core projects for host UI. The core owns
/// every transition and emits states in order; hosts render the current state
/// and never derive auth UI from any other signal.
// A uniffi `Enum` cannot box a variant.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum AuthState {
	/// No active session and no login in progress.
	#[default]
	Disconnected,
	/// A login is in progress: present the pairing deeplink/QR. Leave this
	/// state only on a subsequent emission (connected, failed, or
	/// disconnected after cancellation).
	Pairing {
		/// Wallet pairing deeplink to render as a QR code or open directly.
		deeplink: String,
	},
	/// A session is active.
	Connected(SessionUiInfo),
	/// The last login attempt failed; show the reason and offer a retry.
	LoginFailed {
		/// What kind of failure this was. Hosts branch on this and treat
		/// `reason` as display copy only.
		kind: LoginFailureKind,
		/// Human-readable failure reason.
		reason: String,
	},
	/// The wallet accepted the pairing request and the core is resolving and
	/// persisting the session. Hosts should replace the pairing QR with an
	/// in-progress presentation until a terminal state is emitted.
	Authenticating,
}

/// Why a login attempt failed, for hosts that need to act on the cause rather
/// than only display it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum LoginFailureKind {
	/// The wallet has no free statement-store allowance slot for this period,
	/// so it cannot register the device — which normally holds until the period
	/// rolls over, making a retry a waste of the user's remaining budget.
	///
	/// Recovered heuristically from the wallet's prose, whose wording is not
	/// this workspace's to pin, so treat it as a strong hint rather than a
	/// proof: do not make retry the primary action, but leave a way to reach it.
	NoFreeAllowanceSlots,
	/// Anything else. `reason` carries the detail.
	#[default]
	Other,
}

/// Host auth UI driven by core-owned [`AuthState`] transitions.
pub trait AuthPresenter: Send + Sync {
	/// Observe an auth state change, in transition order. A pairing host
	/// always receives an opening state once the core has restored the
	/// persisted session, `Disconnected` included; later emissions happen
	/// only when the state changes. Default is a no-op for hosts that render
	/// no auth UI.
	fn auth_state_changed(&self, state: AuthState) {
		let _ = state;
	}
}

/// Review shown before a sign-payload request is sent to the paired wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum SignPayloadReview {
	/// Product-account signing request.
	Product {
		/// Product that asked, when the request carries a caller. Absent on a
		/// relayed request, which carries no caller identity. A caller other
		/// than the account's own product is acting under that product's
		/// `context` grant, and the user is the one who has to see that.
		calling_product_id: Option<String>,
		/// Signing request.
		request: HostSignPayloadRequest,
	},
	/// Legacy-account signing request.
	LegacyAccount(HostSignPayloadWithLegacyAccountRequest),
}

/// Review shown before a sign-raw request is sent to the paired wallet.
/// Hosts must display the payload according to `watermarked` and warn that
/// unwatermarked signatures can authorize transactions.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum SignRawReview {
	/// Product-account raw signing request.
	Product {
		/// Product that asked, when the request carries a caller. See
		/// [`SignPayloadReview::Product`].
		calling_product_id: Option<String>,
		/// Raw signing request.
		request: HostSignRawRequest,
		/// Whether the signer applies the `<Bytes>` transaction-payload protection.
		watermarked: bool,
	},
	/// Legacy-account raw signing request.
	LegacyAccount {
		/// Raw signing request.
		request: HostSignRawWithLegacyAccountRequest,
		/// Whether the signer applies the `<Bytes>` transaction-payload protection.
		watermarked: bool,
	},
}

/// Review shown before a product account signs a Statement Store proof
/// payload. Distinct from raw-message signing: the payload is the exact
/// unsigned statement, signed as-is (no `<Bytes>` envelope), so the host must
/// not present it with the raw-signing convention.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct StatementStoreProductSignReview {
	/// Product that asked, when the request carries a caller. See
	/// [`SignPayloadReview::Product`].
	pub calling_product_id: Option<String>,
	/// Product account that will sign the statement payload.
	pub account: ProductAccountId,
	/// Exact unsigned statement payload to be signed.
	pub payload: Vec<u8>,
}

/// Review shown before a transaction-creation request is sent to the paired wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum CreateTransactionReview {
	/// Product-account transaction request.
	Product {
		/// Product that asked, when the request carries a caller. See
		/// [`SignPayloadReview::Product`].
		calling_product_id: Option<String>,
		/// Transaction request.
		payload: ProductAccountTxPayload,
	},
	/// Legacy-account transaction request.
	LegacyAccount(LegacyAccountTxPayload),
}

/// Review shown before a product derives a contextual alias (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct AccountAliasReview {
	/// Product requesting the alias.
	pub calling_product_id: String,
	/// Product-scoped context the alias is bound to.
	pub context: ProductProofContext,
	/// Ring the alias is derived against.
	pub ring_location: RingLocation,
}

/// Review shown before a product creates a ring-VRF proof (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct CreateProofReview {
	/// Product requesting the proof.
	pub calling_product_id: String,
	/// Product-scoped context the proof's alias is bound to.
	pub context: ProductProofContext,
	/// Ring the proof is generated against.
	pub ring_location: RingLocation,
	/// Opaque message bound into the proof.
	pub message: Vec<u8>,
}

/// Review shown before signing an RFC-0023 VRF transcript.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct SignVrfReview {
	/// Product making the request.
	pub calling_product_id: String,
	/// Product account and exact ordered transcript.
	pub request: HostAccountSignVrfRequest,
}

/// Review shown before allocating resources for a product. Names the
/// beneficiary product so the user knows which product receives the
/// (signing-capable) allowance key they are approving.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ResourceAllocationReview {
	/// Product the allocation is requested for.
	pub calling_product_id: String,
	/// Resources to allocate.
	pub resources: Vec<AllocatableResource>,
}

/// Review shown before a product asks to access another product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct AccountAccessReview {
	/// Product currently handling the request.
	pub requesting_product_id: String,
	/// Product whose account is being requested.
	pub target_product_id: String,
}

/// Review shown before a product learns the user's primary identity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct IdentityDisclosureReview {
	/// Product currently handling the request.
	pub product_id: String,
}

/// Review shown before a product resolves its own account subtree over SSO,
/// when the value is not cached and the core must ask the Account Holder.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ProductSubtreeReview {
	/// Product resolving its own account.
	pub product_id: String,
}

/// Review shown before a user-confirmed core action continues.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum UserConfirmationReview {
	/// Sign a SCALE payload with a product or legacy account.
	SignPayload(SignPayloadReview),
	/// Sign raw bytes with a product or legacy account.
	SignRaw(SignRawReview),
	/// Sign a Statement Store proof payload with a product account.
	StatementStoreProductSign(StatementStoreProductSignReview),
	/// Create a transaction with a product or legacy account.
	CreateTransaction(CreateTransactionReview),
	/// Allow a product to derive a contextual alias for a ring.
	AccountAlias(AccountAliasReview),
	/// Allow a product to create a ring-VRF proof for a ring.
	CreateProof(CreateProofReview),
	/// Allow a product to learn the user's primary identity.
	IdentityDisclosure(IdentityDisclosureReview),
	/// Allocate resources for the requesting product.
	ResourceAllocation(ResourceAllocationReview),
	// Index 8 stays unused so a host whose codec is older or newer than the
	// core cannot decode one review as another.
	/// Allow a product to access another product account.
	#[codec(index = 9)]
	AccountAccess(AccountAccessReview),
	/// Sign an RFC-0023 VRF transcript with a product account.
	#[codec(index = 10)]
	SignVrf(SignVrfReview),
	/// Resolve a product's own account subtree over SSO.
	#[codec(index = 11)]
	ProductSubtree(ProductSubtreeReview),
}

/// Local user confirmation UI for sensitive core-owned operations.
#[async_trait]
pub trait UserConfirmation: Send + Sync {
	/// Preserve the lifetime of consent for identity and account disclosures.
	async fn confirm_permission(
		&self,
		review: UserConfirmationReview,
	) -> Result<PermissionDecision, GenericError> {
		Ok(if self.confirm_user_action(review).await? {
			PermissionDecision::AllowAlways
		} else {
			PermissionDecision::Deny
		})
	}

	/// Confirm a reviewed action before the core continues.
	///
	/// The core drops this future when the request behind the review is
	/// withdrawn, and an answer given afterwards reaches nobody. A host should
	/// dismiss its prompt when that happens.
	async fn confirm_user_action(
		&self,
		review: UserConfirmationReview,
	) -> Result<bool, GenericError>;
}

/// Host theme source.
pub trait ThemeHost: Send + Sync {
	/// Emits current theme immediately, then future changes. Hosts with no
	/// named themes report `ThemeName::Default`.
	fn subscribe_theme(&self) -> BoxStream<'static, Result<HostThemeSubscribeItem, GenericError>>;
}

/// Host locale source.
pub trait LocaleHost: Send + Sync {
	/// Emits the currently selected locale immediately, then future changes.
	fn subscribe_locale(&self)
	-> BoxStream<'static, Result<HostLocaleSubscribeItem, GenericError>>;
}

/// Host preimage backend. The core builds, signs, and submits the Bulletin
/// `TransactionStorage.store` transaction itself; the host only owns preimage
/// content retrieval (P2P/IPFS lookup).
#[async_trait]
pub trait PreimageHost: Send + Sync {
	/// Emits current value/miss immediately, then future updates.
	fn lookup_preimage(
		&self,
		key: Vec<u8>,
	) -> BoxStream<'static, Result<Option<Vec<u8>>, GenericError>>;
}

/// Host-implemented adapter through which product Chat calls reach host
/// storage and UI. Optional: a host that omits it leaves Chat requests
/// answered `Unsupported`. See [`OptionalPlatform`].
///
/// The core bounds and screens the product-supplied fields it forwards. Ids,
/// names and icons on `create_chat_room`, `register_chat_bot`,
/// `post_chat_message` and `set_chat_room_footer` are NFC-normalized and rejected for control and
/// bidi characters. Message bodies are bounded and screened but pass through
/// byte-for-byte, keeping line breaks and tabs, so a product reads back the
/// bytes it sent. Counts and byte budgets are enforced, and any URL a host may
/// fetch or open is restricted to `https` or an inline raster image and
/// delivered as the parser resolved it.
///
/// The core screens a URL's shape, not its reachability. `https://127.0.0.1`,
/// `https://[::1]`, a private range and `https://169.254.169.254` (the cloud
/// metadata endpoint) all pass: which networks a host is willing to fetch from
/// depends on where that host runs, and a core that guessed would break a host
/// serving its own media from localhost. A host that fetches these URLs owns
/// that decision. Credentials are the exception and are refused, because
/// `user:pass@` survives resolution into whatever the host fetches and logs.
///
/// `ChatFile::size_bytes` is a product assertion and is not verified against
/// the resource it names. Contextual output escaping, storage limits, and
/// anything a host derives from product-supplied values remain host-owned.
#[async_trait]
pub trait ChatPlatform: Send + Sync {
	/// Create or resolve a product-scoped native chat room. A room that exists
	/// keeps its stored declarations. `icon_url` is the `https` icon of a
	/// caller at the first protocol version, resolved by
	/// [`validate_chat_icon`], or empty. The host draws it when the metadata
	/// has no avatar.
	async fn create_chat_room(
		&self,
		product: &ProductContext,
		request: HostChatCreateRoomRequest,
		icon_url: String,
	) -> Result<HostChatCreateRoomResponse, HostChatCreateRoomError>;

	/// Register or resolve a product-scoped native chat bot. Host-owned in the
	/// same way rooms are.
	async fn register_chat_bot(
		&self,
		product: &ProductContext,
		request: HostChatRegisterBotRequest,
	) -> Result<HostChatRegisterBotResponse, HostChatRegisterBotError>;

	/// Persist a product-authored message in a native chat room. A host that
	/// cannot store a given content variant reports a domain error for it.
	async fn post_chat_message(
		&self,
		product: &ProductContext,
		request: HostChatPostMessageRequest,
	) -> Result<HostChatPostMessageResponse, HostChatPostMessageError>;

	/// Set what a product-scoped room shows below its messages, for the room
	/// as it is now and every later time it is shown. The core has already
	/// checked the product created the room. A room the product never set a
	/// footer on shows the text input.
	async fn set_chat_room_footer(
		&self,
		product: &ProductContext,
		request: HostChatSetRoomFooterRequest,
	) -> Result<(), GenericError>;

	/// Emit the current product-scoped room list and later replacements.
	fn subscribe_chat_rooms(
		&self,
		product: &ProductContext,
	) -> BoxStream<'static, Result<HostChatListSubscribeItem, GenericError>>;

	/// The configuration the product last declared for a room, or the default
	/// configuration when it declared none. The core reads it to decide
	/// whether a room keeps its messages in the host and which queries the
	/// product answers.
	async fn read_chat_room_config(
		&self,
		_product: &ProductContext,
		_room_id: String,
	) -> Result<ChatRoomConfig, GenericError> {
		Ok(ChatRoomConfig::default())
	}

	/// Replace the name, avatar and description of a room.
	async fn set_chat_room_metadata(
		&self,
		_product: &ProductContext,
		_request: HostChatSetRoomMetadataRequest,
	) -> Result<(), GenericError> {
		Err(chat_method_not_served("setChatRoomMetadata"))
	}

	/// Replace how a room shows in the chat list.
	async fn set_chat_room_preview(
		&self,
		_product: &ProductContext,
		_request: HostChatSetRoomPreviewRequest,
	) -> Result<(), GenericError> {
		Err(chat_method_not_served("setChatRoomPreview"))
	}

	/// Delete a room with its messages, cache and declarations, and close its
	/// screen. Delete also, in the same change, each room whose features name
	/// this room in `thread_of`, so that no thread stays hidden for ever. The
	/// core calls it only for a room the product hosts.
	async fn delete_chat_room(
		&self,
		_product: &ProductContext,
		_request: HostChatDeleteRoomRequest,
	) -> Result<(), GenericError> {
		Err(chat_method_not_served("deleteChatRoom"))
	}

	/// Apply one change to the messages of a room. With `persist` the host
	/// stores the change, because it keeps the messages of the room. Without
	/// it the host applies the change only to an open room screen and the chat
	/// list preview. The host answers `UnknownMessage`, `DuplicateMessage`
	/// and, for an edit of a message the product did not write, `NotOwner`.
	async fn update_chat_feed(
		&self,
		_product: &ProductContext,
		_request: HostChatUpdateFeedRequest,
		_persist: bool,
	) -> Result<(), HostChatUpdateFeedError> {
		Err(HostChatUpdateFeedError::Unknown {
			reason: chat_method_not_served("updateChatFeed").reason,
		})
	}

	/// The names of the Chat features this host serves from end to end, from
	/// [`CHAT_FEATURE_NAMES`]. The core removes each name it cannot back and
	/// each name it does not know. A host that lists nothing serves the
	/// features of Chat Extension v2 only.
	async fn chat_capabilities(
		&self,
		_product: &ProductContext,
	) -> Result<ChatFeatureNames, GenericError> {
		Ok(ChatFeatureNames::default())
	}

	/// The optional features the product last declared for a room, or the
	/// default features when it declared none.
	async fn read_chat_room_features(
		&self,
		_product: &ProductContext,
		_room_id: String,
	) -> Result<ChatRoomFeatures, GenericError> {
		Ok(ChatRoomFeatures::default())
	}

	/// Replace the optional features of a room whole and keep them across
	/// restarts. The host keeps a feature it does not serve, so that a later
	/// build of the host applies it. The core has already checked the product
	/// hosts the room, and the thread parent.
	async fn set_chat_room_features(
		&self,
		_product: &ProductContext,
		_request: HostChatSetRoomFeaturesRequest,
	) -> Result<(), GenericError> {
		Err(chat_method_not_served("setChatRoomFeatures"))
	}

	/// Show, change or remove the call of a room. The core has already
	/// checked that the host serves the call.
	async fn set_chat_room_call(
		&self,
		_product: &ProductContext,
		_request: HostChatSetRoomCallRequest,
	) -> Result<(), GenericError> {
		Err(chat_method_not_served("setChatRoomCall"))
	}

	/// The mute and archive state of each room of the product that the user
	/// muted or archived. The core sends it to a product when its
	/// `room_events_subscribe` starts.
	async fn read_chat_room_user_states(
		&self,
		_product: &ProductContext,
	) -> Result<ChatRoomUserStates, GenericError> {
		Ok(ChatRoomUserStates { rooms: Vec::new() })
	}

	/// The bot ids of the personas the product registered.
	async fn read_chat_bots(&self, _product: &ProductContext) -> Result<ChatBotIds, GenericError> {
		Ok(ChatBotIds::default())
	}
}

/// The mute and archive state the user gave one room of a product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ChatRoomUserState {
	/// The room.
	pub room_id: String,
	/// Milliseconds since the Unix epoch when the mute ends. `u64::MAX` mutes
	/// the room with no end. `None`: the room is not muted.
	pub muted_until: Option<u64>,
	/// The room is in the archive of the user.
	pub archived: bool,
}

/// The Chat feature names a host lists.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
pub struct ChatFeatureNames {
	/// Names from [`CHAT_FEATURE_NAMES`].
	pub names: Vec<String>,
}

/// The bot ids of the personas of a product.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
pub struct ChatBotIds {
	/// One bot id for each persona.
	pub bot_ids: Vec<String>,
}

/// The mute and archive state of each room of a product that the user muted
/// or archived.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
pub struct ChatRoomUserStates {
	/// One entry for each such room.
	pub rooms: Vec<ChatRoomUserState>,
}

impl ChatRoomUserState {
	/// Whether the room is muted at `now_ms`, milliseconds since the Unix
	/// epoch.
	pub fn muted_at(&self, now_ms: u64) -> bool {
		self.muted_until.is_some_and(|until| until > now_ms)
	}
}

/// The failure of a chat platform method a host does not serve yet.
fn chat_method_not_served(method: &str) -> GenericError {
	GenericError { reason: format!("this host does not serve {method}") }
}

/// A funding session as the host overlay needs it to open on the right screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingPresentation {
	/// Session id.
	pub intent: String,
	/// Which way value moves, which decides the screen the overlay opens on.
	pub direction: FundingDirection,
	/// Amount the caller asked for, or `None` to let the user choose.
	pub amount: Option<u128>,
}

/// How the user left the funding overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum FundingPresentOutcome {
	/// The user went ahead; the session runs on without the overlay.
	Started,
	/// The user closed the overlay before starting; the core discards the
	/// session.
	Dismissed,
}

/// Host-implemented funding surface: the native overlay a funding session
/// runs in. Optional: a host that omits it leaves funding requests answered
/// `Unsupported`.
#[async_trait]
pub trait FundingPlatform: Send + Sync {
	/// Show the funding overlay for a session that `product` opened, or the
	/// host itself when `product` is `None`.
	async fn present_funding(
		&self,
		product: Option<&ProductContext>,
		session: FundingPresentation,
	) -> Result<FundingPresentOutcome, GenericError>;

	/// Observe a session's status change, for host UI such as the in-flight
	/// pill.
	fn funding_session_changed(&self, intent: String, status: HostFundingStatusSubscribeItem) {
		let _ = (intent, status);
	}
}

/// Host-implemented balance view: what a payment request can spend right now,
/// the figure the host checks a payment against. Optional: a host that omits
/// it leaves balance subscriptions answered `Unsupported`.
///
/// The balance is the user's, per purse, not the product's: every product
/// following a purse sees the same figure. The core asks for the product's
/// balance access before calling here, and checks it again before passing on
/// each balance the stream emits, so a revoked grant ends the stream with
/// `PermissionDenied`.
pub trait BalancePlatform: Send + Sync {
	/// Emit the balance of `purse` (`None` for the main purse) now and on
	/// every change.
	fn subscribe_balance(
		&self,
		product: &ProductContext,
		purse: Option<u32>,
	) -> BoxStream<'static, Result<HostPaymentBalanceSubscribeItem, HostPaymentBalanceSubscribeError>>;
}

/// Host-implemented top-up engine: claims a source's funds into the user's
/// balance through the host's coinage onboarding. Optional: a host that omits
/// it leaves top-ups answered `Unsupported`.
///
/// The core validates the source keys and hashes the product into the id
/// before calling, so ids never collide across products. The host owns
/// retries, partial claims and persistence.
#[async_trait]
pub trait TopUpPlatform: Send + Sync {
	/// Start a top-up. Returns once the host has accepted it.
	async fn top_up(
		&self,
		product: &ProductContext,
		request: HostPaymentTopUpRequest,
	) -> Result<(), HostPaymentTopUpError>;

	/// Emit a top-up's current status and every later one, ending after a
	/// terminal status.
	fn subscribe_top_up_status(
		&self,
		product: &ProductContext,
		id: Bytes32,
	) -> BoxStream<
		'static,
		Result<HostPaymentTopUpStatusSubscribeItem, HostPaymentTopUpStatusSubscribeError>,
	>;
}

/// Host-implemented payment engine: pays from the user's balance to an
/// account, once the user approves. Optional: a host that omits it leaves
/// payment requests answered `Unsupported`.
///
/// The core hashes the product into the id before calling, so ids never
/// collide across products. The host owns the approval sheet, the transfer
/// and its persistence.
#[async_trait]
pub trait PaymentPlatform: Send + Sync {
	/// Ask the user to approve `request`. Returns once the user has decided:
	/// `Ok` when they authorized it and the host took it on; the payment's
	/// outcome arrives through its status.
	async fn request_payment(
		&self,
		product: &ProductContext,
		request: HostPaymentRequest,
	) -> Result<(), HostPaymentError>;

	/// Emit a payment's current status and every later one, ending after a
	/// terminal status.
	fn subscribe_payment_status(
		&self,
		product: &ProductContext,
		id: Bytes32,
	) -> BoxStream<'static, Result<HostPaymentStatusSubscribeItem, HostPaymentStatusSubscribeError>>;
}

/// Host-implemented adapter through which product Pocket calls reach the
/// host's card collection. Optional: a host that omits it leaves Pocket
/// requests answered `Unsupported`. See [`OptionalPlatform`].
///
/// The host owns the collection: it decides which cards are privileged and
/// keeps each card's newest face. A face does not cross this boundary.
#[async_trait]
pub trait PocketPlatform: Send + Sync {
	/// Emit the calling product's current cards and every later replacement.
	fn subscribe_pocket_cards(
		&self,
		product: &ProductContext,
	) -> BoxStream<'static, Result<HostPocketListSubscribeItem, GenericError>>;

	/// Remove one of the calling product's cards. Removing an absent card
	/// succeeds; a privileged card is refused with `Privileged`.
	async fn remove_pocket_card(
		&self,
		product: &ProductContext,
		request: HostPocketRemoveCardRequest,
	) -> Result<(), HostPocketRemoveCardError>;
}

/// Host-implemented adapter that holds a product's next-game reminder.
/// Optional: a host that omits it leaves Game requests answered `Unsupported`.
/// See [`OptionalPlatform`].
///
/// The core serves only the game product and refuses a start that is not in
/// the future before it calls here; it asks for no per-product consent. The
/// host asks the OS for what the reminder needs, rings an alarm where the OS
/// allows one and delivers an ordinary notification otherwise, and may add the
/// game to the user's calendar. A host keeps one reminder per product: a
/// schedule replaces the reminder the same product already holds and leaves
/// other products' reminders alone. The host keeps each reminder across app
/// kill and device reboot and drops it once its game has started.
#[async_trait]
pub trait GamePlatform: Send + Sync {
	/// Hold `starts_at` (Unix milliseconds, UTC) as the product's reminder,
	/// replacing any it holds. An error, including an OS that allows neither
	/// alarms nor notifications, reaches the product as a host failure.
	async fn schedule_game_reminder(
		&self,
		product: &ProductContext,
		starts_at: u64,
	) -> Result<(), GenericError>;

	/// Drop the product's reminder. Idempotent: dropping none succeeds.
	async fn cancel_game_reminder(&self, product: &ProductContext) -> Result<(), GenericError>;
}

/// What the operating system currently says about a device capability.
///
/// Distinct from [`PermissionAuthorizationStatus`], which is the product-scoped
/// decision the user made through TrUAPI. The two answer different questions
/// and are combined rather than substituted: a capability is usable only when
/// the product holds a grant *and* the OS still allows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum DevicePermissionStatus {
	/// The OS grants this capability to the host application.
	Granted,
	/// The OS refuses it. Prompting again will not help; the user has to
	/// change it in system settings.
	Denied,
	/// The OS has not been asked yet, either because it never was or because
	/// it reset the grant. The core does not treat this as a refusal: the OS
	/// puts its own dialog up when the capability is used, and the core has no
	/// way to reach that dialog without also re-asking the product's question.
	NotDetermined,
	/// This platform has no OS-level gate for the capability, so the
	/// product-scoped decision alone governs it.
	NotApplicable,
}

/// Live OS permission state, read without prompting.
///
/// A product-scoped grant is persisted once and never expires, but the OS
/// grant behind it can be revoked in system settings, suspended by device
/// policy, or reset by the platform — Android auto-resets runtime permissions
/// for apps that go unused. Without this capability the core keeps answering
/// from the stored grant alone and tells a product `granted` for a capability
/// the OS has since taken away.
///
/// This is deliberately separate from [`Permissions::device_permission`]: that
/// call may show UI, so it cannot be used to re-check a decision the user has
/// already made without prompting them again on every request.
#[async_trait]
pub trait PermissionStatusHost: Send + Sync {
	/// Current OS status of a device capability. Must not prompt.
	async fn device_permission_status(
		&self,
		request: HostDevicePermissionRequest,
	) -> Result<DevicePermissionStatus, GenericError>;
}

/// What the host did with a request to show or hide the expanded card face.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum ExpandedCardFaceOutcome {
	/// The face is now in the requested state, including when it already was.
	Applied,
	/// The Widget is not shown under its card right now.
	NotPresented,
	/// The user is moving the face, so the request had no effect.
	UserMoving,
	/// The host cannot move the face at all.
	Unsupported,
}

/// Host control of the card face drawn above an opened card's Widget.
///
/// Carried per connection on `ConnectionAdapters`, because the host owns one
/// drawer per product execution. A connection without one tells products
/// `Unsupported`.
#[async_trait]
pub trait ExpandedCardHost: Send + Sync {
	/// Show (`true`) or hide (`false`) the face above the calling Widget.
	async fn set_expanded_card_face_shown(
		&self,
		shown: bool,
	) -> Result<ExpandedCardFaceOutcome, GenericError>;
}

/// Host store for a product's pending operations, which the host uses to keep
/// the product's worker runtime alive. Reached only through the `Worker`
/// protocol trait, so non-worker products never call these.
#[async_trait]
pub trait ProductOperations: Send + Sync {
	/// Record a pending operation. `label` is a host log and UI hint, empty
	/// when the product gave none.
	///
	/// The returned id must be unique among this product's open operations.
	/// The core keys the worker reference an operation holds by that id, so an
	/// id already open for the product records nothing the second time, and
	/// ending it once drops the demand both were holding. Ids may repeat
	/// across products, and may be reused once an operation has ended.
	async fn begin_operation(
		&self,
		product: &ProductContext,
		label: String,
	) -> Result<HostWorkerBeginOperationResponse, HostWorkerOperationError>;

	/// Remove a pending operation. Idempotent: an unknown or already-ended id
	/// returns `Ok`, so a retry after an ambiguous failure is safe.
	async fn end_operation(
		&self,
		product: &ProductContext,
		id: u32,
	) -> Result<(), HostWorkerOperationError>;
}

/// Contact handles the core needs turned back into accounts, and the key they
/// were minted under.
///
/// A handle is `BLAKE2b-256(key = handle_key, message = account)`, the account
/// being its 32 raw bytes. The host holds the accounts, so it is the one that
/// can match: hash each contact's account under `handle_key`, or keep that hash
/// as an indexed column for the session, and look the handles up.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostContactLookup {
	/// The key every handle here was minted under. Per session, and never
	/// given to a product.
	pub handle_key: Bytes32,
	/// The handles to resolve, in the order the answer must follow.
	pub handles: Vec<Bytes32>,
}

/// The host's answer to a [`HostContactLookup`].
///
/// A named wrapper because the callback emitter cannot return a bare `Vec`.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostContactMatches {
	/// One entry per requested handle, in order: the contact's account, or
	/// `None` when no current contact hashes to it.
	pub accounts: Vec<Option<Bytes32>>,
}

/// How a host's contact picker ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum HostContactPick {
	/// The user chose this account.
	///
	/// Consumed by the core to mint the product-facing handle and never
	/// forwarded to a product: it is the person's real account, and the handle
	/// exists precisely so a product does not receive it.
	Picked {
		/// The chosen contact's account.
		account: Bytes32,
	},
	/// The user closed the picker without choosing.
	Dismissed,
	/// The user has no contacts, so the host drew nothing.
	NoContacts,
	/// This host resolves contacts but cannot present a picker. The core
	/// answers the product `Unsupported`, so it can tell "try again later"
	/// apart from "this host will never pick".
	Unsupported,
}

/// Host-owned contact picker, drawn from the chat lists the host's chat
/// extensions hold.
///
/// Optional, and listed on [`OptionalPlatform`] as [`ChatPlatform`] is.
///
/// The host owns the UI and the list. It draws the names, so nothing it renders
/// reaches the product, and the list never crosses to the core either: the core
/// asks only about the handles a transaction names. A host omits contacts the
/// user has blocked, from the picker and from lookups alike.
#[async_trait]
pub trait ContactsPlatform: Send + Sync {
	/// Resolve `lookup.handles` to the contacts they name.
	///
	/// The one method a host has to write. Answer one entry per handle, in
	/// order, with `None` for a handle no current contact hashes to — a
	/// removed or blocked contact, or a handle a product made up. The core
	/// re-hashes every account returned and refuses one that does not match
	/// its handle, so a wrong answer is caught rather than trusted.
	async fn contacts(
		&self,
		lookup: &HostContactLookup,
	) -> Result<HostContactMatches, GenericError>;

	/// Present the contact picker on behalf of `product` and return the user's
	/// choice.
	///
	/// Defaults to [`HostContactPick::Unsupported`], so a Rust host that
	/// implements [`Self::contacts`] alone still compiles and its products get
	/// a truthful answer rather than a dismissal they would retry forever.
	///
	/// A JS host reaches the same answer by another route: the generated
	/// surface types this method optional, but a capability group counts as
	/// served only when every callback in it is present, so omitting this one
	/// makes the whole group absent and `contacts.pick` answers `Unsupported`
	/// before any of it is reached.
	///
	/// The core cannot draw UI, so a selection has to come from the host; the
	/// whole point is that the host renders the names rather than shipping
	/// them to the product. `product` is passed so the host can say who is
	/// asking; it is not a filter. A host with no contacts answers
	/// [`HostContactPick::NoContacts`] instead of drawing an empty overlay.
	async fn pick_contact(
		&self,
		_product: &ProductContext,
	) -> Result<HostContactPick, GenericError> {
		Ok(HostContactPick::Unsupported)
	}
}

/// How the host's scanner ended.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum HostScan {
	/// The user scanned a code the request accepts.
	Scanned {
		/// The code's content.
		text: String,
		/// The code's format.
		format: CodeFormat,
	},
	/// The user closed the viewfinder without scanning.
	Dismissed,
	/// The device has no camera, or the user refused the host application one.
	/// The host has already told the user how to turn it on.
	CameraUnavailable,
	/// The requesting App or Widget is not the screen the user sees, so the
	/// host opened nothing. A Worker's request was already checked by the core
	/// and is never answered this way.
	NotVisible,
}

/// Host-owned viewfinder for QR codes and barcodes. Optional. The Swift and
/// Kotlin bridges and the JS `scanner` callbacks follow the same rules.
///
/// - Title the viewfinder with the product id. Show `request.hint` under it as the product's words.
/// - Pass every code the camera reads to a `ScanFilter` built from `request`, and never follow a
///   scanned link.
/// - Answer [`HostScan::NotVisible`] for an App or Widget that is not on screen. A Worker reaching
///   the host already passed the core's tap check.
/// - Close the viewfinder when the core drops the future, and close any still open before opening
///   another. A JS host is not told about a drop.
#[async_trait]
pub trait ScannerPlatform: Send + Sync {
	/// Open the viewfinder on behalf of `product` and wait for the user.
	async fn scan_code(
		&self,
		product: &ProductContext,
		request: &HostScannerScanRequest,
	) -> Result<HostScan, GenericError>;
}

/// Host-owned directory of the usernames of the network. Optional; a host
/// without one leaves every product search `Unsupported`.
///
/// - Search the way the host's own contact search does, and give at most 20 usernames.
#[async_trait]
pub trait UsernamesPlatform: Send + Sync {
	/// Find the usernames that start with `prefix`.
	async fn search_usernames(
		&self,
		product: &ProductContext,
		prefix: &str,
	) -> Result<Vec<String>, GenericError>;
}

/// Host-run media of the calls of chat rooms (WebRTC with the relays of the
/// host). Optional; a host without it leaves every product `callMedia` call
/// `Unsupported`.
///
/// - Start only for a call that the room shows as `Active`, and show the call screen of the host.
/// - Give each media signal of the host to the product as `ChatRoomEvent::CallSignal`.
/// - Keep a signal that comes before its call starts, and give it to the call when it starts.
/// - Send `ChatRoomEvent::CallEnded` when the user ends the call on the call screen.
#[async_trait]
pub trait CallMediaPlatform: Send + Sync {
	/// Start the media of `call_id` of `room_id`.
	async fn start_call_media(
		&self,
		product: &ProductContext,
		room_id: &str,
		call_id: &str,
		video: bool,
		offerer: bool,
	) -> Result<(), GenericError>;

	/// A media signal of the other side of `call_id`.
	async fn call_media_signal(
		&self,
		product: &ProductContext,
		room_id: &str,
		call_id: &str,
		signal: Vec<u8>,
	) -> Result<(), GenericError>;

	/// Stop the media of `call_id`.
	async fn stop_call_media(
		&self,
		product: &ProductContext,
		room_id: &str,
		call_id: &str,
	) -> Result<(), GenericError>;
}

/// The push service that a device token belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum HostPushService {
	/// Apple Push Notification service: the token is the hex of the APNs
	/// device token.
	Apns,
	/// Firebase Cloud Messaging: the token is the FCM registration token.
	Fcm,
}

/// One push that the host gives to its push relay.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostPushDelivery {
	/// The push service of the receiving device.
	pub service: HostPushService,
	/// The push token of the receiving device.
	pub token: String,
	/// The id of the room on the receiving device.
	pub push_id: String,
	/// The hex of the encrypted notification: a 24-byte nonce, then the
	/// XChaCha20-Poly1305 ciphertext for a product notification, or the
	/// person chat's own encryption for a chat push.
	pub message: String,
	/// Whether the push goes out as a VoIP push, which iOS hands to CallKit.
	pub voip: bool,
}

/// How the host shows the chat pushes of one contact while no worker runs:
/// the push id the contact sends to this device under, the room it opens, the
/// title of the notification, and the key the payload is sealed with.
#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostChatPushRoute {
	/// 64 lowercase hex characters, as the push carries the id.
	pub push_id: String,
	/// The room of the chat product the notification opens.
	pub room_id: String,
	/// The title of the notification: the contact's name.
	pub title: String,
	/// The 32-byte ChaCha20-Poly1305 key of the payload, nonce first.
	#[debug("<redacted>")]
	pub key: Vec<u8>,
}

/// The push tokens of this device, which a contact needs to notify it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostPushTokens {
	/// The push service of the tokens.
	pub service: HostPushService,
	/// The push token, when the device has one now: the hex of the APNs
	/// device token, or the FCM registration token.
	pub token: Option<String>,
	/// The hex of the PushKit VoIP token, on iOS when it has one.
	pub voip_token: Option<String>,
}

/// Host services of the person-to-person chat product that only the app
/// knows or keeps: the HOP nodes of the network's Bulletin chain (its chain
/// registry's `externalApi.hop`), the devices paired with the wallet, the
/// chat's contacts for the rest of the app, and the native chat history it
/// staged for the chat product. Optional; a host without it leaves the
/// `PersonChat` HOP, contact and history calls `Unsupported`.
#[async_trait]
pub trait PersonChatPlatform: Send + Sync {
	/// The `wss://` URLs of the HOP nodes the app trusts: a sender picks one,
	/// a receiver claims only from one of them.
	async fn hop_nodes(&self) -> Result<Vec<String>, GenericError>;

	/// The devices paired with this wallet device over SSO, as the native
	/// apps announce them to contacts (`DeviceAdded`): the device's statement
	/// account and the encryption key of its SSO session.
	async fn paired_devices(&self) -> Result<Vec<HostPairedDevice>, GenericError>;

	/// Replace the chat contacts of `product`, which the app shows outside
	/// the chat: in the wallet, as caller names, and in the contact picker of
	/// other products.
	async fn set_contacts(
		&self,
		product: &ProductContext,
		contacts: Vec<HostChatContact>,
	) -> Result<(), GenericError>;

	/// At most `max_len` bytes from `offset` of the native chat history the
	/// host staged for `product` when it moved its native chat over, and the
	/// size of the whole export; a `total` of 0 when it staged none.
	async fn history_export(
		&self,
		product: &ProductContext,
		offset: u64,
		max_len: u32,
	) -> Result<HostHistoryExportChunk, GenericError>;

	/// `product` imported the staged history: the host may drop it.
	async fn history_imported(&self, product: &ProductContext) -> Result<(), GenericError>;
}

/// A part of the native chat history a host staged for the chat product.
#[derive(Clone, PartialEq, Eq, derive_more::Debug)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostHistoryExportChunk {
	/// The size of the whole export in bytes; 0 when there is none.
	pub total: u64,
	/// The bytes from the requested offset.
	#[debug("{} bytes", data.len())]
	pub data: Vec<u8>,
}

/// A contact of the chat product, for the rest of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostChatContact {
	/// The contact's identity account.
	pub account_id: [u8; 32],
	/// The contact's username, when the chat knows it.
	pub username: Option<String>,
	/// The room of the chat product that holds the chat with the contact.
	pub room_id: String,
	/// Whether the user blocked the contact.
	pub blocked: bool,
}

/// A device paired with this wallet device over SSO.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostPairedDevice {
	/// The device's statement account.
	pub statement_account_id: [u8; 32],
	/// The X25519 key the device decrypts chat with: its SSO encryption key.
	pub encryption_public_key: [u8; 32],
	/// The name the user knows the device by.
	pub name: String,
}

/// Host-owned push service: the push token of the device, the push routes of
/// the rooms, and a relay that sends a push to another device. Optional; a
/// host without one leaves every product `push` call `Unsupported`.
///
/// - Keep the id and the key of each route where the notification receiver of the host reads them.
/// - Send each delivery to the push relay with the credentials of the host.
#[async_trait]
pub trait PushPlatform: Send + Sync {
	/// Give `delivery` to the push relay. Answers whether the push service took
	/// it.
	async fn send_push(
		&self,
		product: &ProductContext,
		delivery: HostPushDelivery,
	) -> Result<bool, GenericError>;

	/// The push tokens of this device now.
	async fn push_tokens(&self) -> Result<HostPushTokens, GenericError>;

	/// Replace the chat push routes of `product` with `routes`, where the
	/// host's notification receiver reads them.
	async fn set_chat_push_routes(
		&self,
		product: &ProductContext,
		routes: Vec<HostChatPushRoute>,
	) -> Result<(), GenericError>;
}

/// Combined platform interface. A host must provide every capability trait
/// listed here. Members marked optional may be omitted; the core answers their
/// product calls with `Unsupported`. See [`OptionalPlatform`].
pub trait Platform:
	Navigation
	+ Notifications
	+ Permissions
	+ Features
	+ ProductStorage
	+ CoreStorage
	+ ChainProvider
	+ AuthPresenter
	+ UserConfirmation
	+ ThemeHost
	+ LocaleHost
	+ PreimageHost
	+ ProductOperations
{
}

impl<T> Platform for T where
	T: Navigation
		+ Notifications
		+ Permissions
		+ Features
		+ ProductStorage
		+ CoreStorage
		+ ChainProvider
		+ AuthPresenter
		+ UserConfirmation
		+ ThemeHost
		+ LocaleHost
		+ PreimageHost
		+ ProductOperations
{
}

/// Capability traits a host may serve but is not required to. A host that
/// omits one is not broken: the core answers the corresponding product calls
/// with `Unsupported`. Codegen reads this list to emit each capability as an
/// optional group on the host-callback surface.
pub trait OptionalPlatform:
	ChatPlatform
	+ ContactsPlatform
	+ PermissionStatusHost
	+ PocketPlatform
	+ GamePlatform
	+ ScannerPlatform
{
}

impl<T> OptionalPlatform for T where
	T: ChatPlatform
		+ ContactsPlatform
		+ PermissionStatusHost
		+ PocketPlatform
		+ GamePlatform
		+ ScannerPlatform
{
}
