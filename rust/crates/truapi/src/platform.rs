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

pub use truapi_provider::{
	ProviderError,
	platform::{ChainProvider, JsonRpcConnection},
};

use truapi::Bytes32;

/// In-memory mock platform for tests and host simulators (enable the `mock` feature).
#[cfg(feature = "mock")]
pub mod mock;

/// Strict host-only migration of the pinned Epoca 0.26 storage keyspace.
pub mod epoca026;

pub use crate::{
	dotns_name::{DOTNS_TLDS, MAX_DNS_LABEL_LEN, has_dotns_tld, is_dns_label},
	product_id::{
		PRODUCT_ID_MAX_BYTES, ProductId, ProductIdError, REMOTE_PERMISSION_TRUSTED_LABELS,
	},
};

use truapi::{
	latest::{
		AllocatableResource, AvatarRect, ChainIdentifier, ChatAction, ChatActions,
		ChatCustomMessage, ChatFile, ChatMedia, ChatMessageContent, ChatReaction, ChatRichText,
		CodeFormat, DerivationIndex, FundingDirection, FundingQuote, FundingQuoteRefusal,
		GenericError, HostChatCreateRoomError, HostChatCreateRoomRequest,
		HostChatCreateRoomResponse, HostChatListSubscribeItem, HostChatPostMessageError,
		HostChatPostMessageRequest, HostChatPostMessageResponse, HostChatRegisterBotError,
		HostChatRegisterBotRequest, HostChatRegisterBotResponse, HostChatSetRoomFooterRequest,
		HostDevicePermissionRequest, HostFeatureSupportedRequest, HostFeatureSupportedResponse,
		HostFundingStatusSubscribeItem, HostLocalStorageChangeItem, HostLocaleSubscribeItem,
		HostNativeChatAttachmentMetadata, HostNavigateToError, HostPaymentBalanceSubscribeError,
		HostPaymentBalanceSubscribeItem, HostPaymentError, HostPaymentRequest,
		HostPaymentStatusSubscribeError, HostPaymentStatusSubscribeItem, HostPaymentTopUpError,
		HostPaymentTopUpRequest, HostPaymentTopUpStatusSubscribeError,
		HostPaymentTopUpStatusSubscribeItem, HostPlatform, HostPocketListSubscribeItem,
		HostPocketRemoveCardError, HostPocketRemoveCardRequest,
		HostProfilePlaceContactAvatarsError, HostProfilePresentError, HostProfilePresentRequest,
		HostPushNotificationRequest, HostPushNotificationResponse, HostScannerScanRequest,
		HostSignPayloadRequest, HostSignPayloadWithLegacyAccountRequest, HostSignRawRequest,
		HostSignRawWithLegacyAccountRequest, HostThemeSubscribeItem,
		HostWorkerBeginOperationResponse, HostWorkerOperationError, LegacyAccountTxPayload,
		NftPurseOwner, ProductAccountId, ProductAccountTxPayload, ProductProofContext,
		RemotePermission, RemotePermissionRequest, RingLocation,
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
#[derive(Clone)]
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
	/// Trusted Coinage asset instance for runtimes with instance-scoped assets.
	/// Required by those runtimes; `None` preserves the legacy Coinage ABI.
	/// This is not a purse derivation identifier.
	pub coinage_instance_id: Option<u32>,
}

/// Product identity attached to one product-facing TrUAPI connection.
///
/// A host may create multiple product runtimes from the same long-lived host
/// runtime, each with its own product context.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ProductContext {
	/// Product identifier used for account derivation and product-scoped
	/// storage/permission namespaces.
	///
	/// Host-spec C.7 defines accepted product id forms:
	/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/C-account-derivation.md?plain=1#L109-L128>
	pub product_id: ProductId,
	/// Trusted kind of executable attached to this connection by the host.
	pub execution_kind: ExecutableKind,
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
pub enum ExecutableKind {
	/// Visible full-page entrypoint such as `app/index.html`.
	#[default]
	App,
	/// Visible embedded surface such as a dashboard card.
	Widget,
	/// Headless executable the host runs while a modality holds a reference
	/// to it; the only kind that may serve the Chat modality.
	Worker,
}

impl ExecutableKind {
	/// The dotNS prefix used for this kind's executable records.
	pub fn dotns_label(self) -> &'static str {
		match self {
			Self::App => "app",
			Self::Widget => "widget",
			Self::Worker => "worker",
		}
	}
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
			coinage_instance_id: None,
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
	/// A product context for an app.
	pub fn new(product_id: ProductId) -> Self {
		Self::new_with_execution(product_id, ExecutableKind::App)
	}

	/// A product context for a host-selected executable kind.
	pub fn new_with_execution(product_id: ProductId, execution_kind: ExecutableKind) -> Self {
		Self { product_id, execution_kind }
	}
}

/// Hosts available to every product unless a stored permission decision blocks them.
pub const BLESSED_REMOTE_DOMAINS: &[&str] = &["fonts.googleapis.com", "fonts.gstatic.com"];

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
/// Screened harder than a display name: an identifier is matched, not read, so it
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
		Content::Actions(actions) => {
			if actions.actions.len() > CHAT_ACTIONS_MAX {
				return Err(ChatFieldError::TooMany { field: "actions", limit: CHAT_ACTIONS_MAX });
			}
			Content::Actions(ChatActions {
				text: actions.text.map(|t| validate_chat_body("text", &t)).transpose()?,
				actions: validate_chat_actions(actions.actions)?,
				layout: actions.layout,
			})
		},
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
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), RuntimeConfigValidationError> {
	if value.trim().is_empty() {
		return Err(RuntimeConfigValidationError::EmptyField { field });
	}
	Ok(())
}

/// Runtime config validation error.
#[derive(
	Debug, Clone, PartialEq, Eq, derive_more::Display, derive_more::Error, derive_more::From,
)]
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
	/// Product id did not parse.
	#[display("{_0}")]
	#[from]
	InvalidProductId(#[error(not(source))] ProductIdError),
	/// Network suffix was not a supported dotNS TLD.
	#[display("network_suffix must be a supported dotNS TLD, got {network_suffix:?}")]
	InvalidNetworkSuffix {
		/// Actual network suffix value.
		network_suffix: String,
	},
}

const PRODUCT_STORAGE_KEY_PREFIX: &str = "truapi:product-storage:v1:";

/// Decoded product scope and product-owned key used by [`ProductStorage`].
///
/// The string representation remains the host callback ABI. Native hosts that
/// need separate backing stores can decode it without duplicating the wire
/// format.
#[derive(Clone, Debug, PartialEq)]
pub struct ProductStorageKey {
	product_id: ProductId,
	key: String,
}

impl ProductStorageKey {
	/// Build a key owned by `product_id`.
	pub fn new(product_id: ProductId, key: impl Into<String>) -> Self {
		Self { product_id, key: key.into() }
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
		let product_id = product_id.parse::<ProductId>().map_err(|error| error.to_string())?;
		Ok(Self::new(product_id, key))
	}

	/// Product identifier owning this storage key.
	pub fn product_id(&self) -> &ProductId {
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
			self.product_id.as_str().len(),
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
#[async_trait::async_trait]
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
#[async_trait::async_trait]
pub trait Navigation: Send + Sync {
	/// Open the given URL in the system browser.
	async fn navigate_to(&self, url: String) -> Result<(), HostNavigateToError>;
}

/// Trusted host receiving scope, derived from verified artifact and account state.
/// It must remain available after the product execution closes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ReceivingAuthority {
	/// Verified product identifier.
	pub product_id: String,
	/// Opaque stable account identity, encoded as 32-byte lowercase hex.
	pub account: String,
	/// Host-selected network environment.
	pub environment: String,
	/// Verified artifact digest, encoded as 32-byte lowercase hex.
	pub artifact: String,
	/// Host-selected receiving chain genesis, encoded as 32-byte lowercase hex.
	pub genesis: String,
	/// Host logout, account and artifact fence.
	pub generation: u64,
	/// Current OS notification permission.
	pub os_permission: bool,
	/// Whether the explicitly selected transport is ready.
	pub transport_ready: bool,
}

/// Host-only durable registration awaiting transport synchronization.
/// Routes and authority identifiers stay local, not in provider payloads.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, serde::Serialize, serde::Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ReceivingRegistration {
	/// Trusted scope under which consent was recorded.
	pub authority: ReceivingAuthority,
	/// Durable local revision, distinct from provider token revisions.
	pub revision: u64,
	/// Whether this revision enrolls watches or revokes them.
	pub enabled: bool,
	/// Locally approved source filters and activation routes.
	pub watches: Vec<crate::latest::ReceivingWatch>,
	/// Whether this revision still needs synchronization.
	pub sync_pending: bool,
}

/// Deliver push notifications.
#[async_trait::async_trait]
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

	/// Resolve trusted authority without prompting or consulting transport.
	/// The resident platform returns current host scope, even with product UI closed.
	/// A product execution's platform returns the immutable scope captured when
	/// that verified execution opened, never a replacement artifact/account's scope.
	/// Returning `None` explicitly advertises unsupported receiving.
	async fn receiver_authority(
		&self,
		product_id: &str,
	) -> Result<Option<ReceivingAuthority>, GenericError> {
		let _ = product_id;
		Ok(None)
	}

	/// Request distinct consent for this authority and full watch scope.
	async fn receiver_consent(
		&self,
		authority: ReceivingAuthority,
		watches: Vec<crate::latest::ReceivingWatch>,
	) -> Result<bool, GenericError> {
		let _ = (authority, watches);
		Err(GenericError { reason: "background receiving unsupported".into() })
	}

	/// Wake the host's asynchronous synchronization loop, never await network I/O.
	async fn receiver_changed(&self) -> Result<(), GenericError> {
		Err(GenericError { reason: "background receiving unsupported".into() })
	}

	/// Forward receiving actions to the single host-owned receiver, if external.
	/// The host must bind `product_id` to the trusted execution, not page input.
	/// Payloads are latest SCALE requests for actions 2..7; the response encodes
	/// `Result<latest response, HostNotificationReceivingError>` without a version tag.
	/// `None` selects this runtime's resident engine. An unavailable external
	/// owner must return an error, never `None`, to avoid a second persistent writer.
	async fn receiver_command(
		&self,
		product_id: String,
		action: u8,
		payload: Vec<u8>,
	) -> Result<Option<Vec<u8>>, GenericError> {
		let _ = (product_id, action, payload);
		Ok(None)
	}

	/// Return at most 32 pending activations, ordered by sequence, without
	/// consuming them. The embedding host binds this platform to the verified
	/// product, authenticated account and environment; none is caller input.
	/// Admit only routes starting with exactly one slash, with no backslashes
	/// or control characters. Polling must not request permissions or enroll
	/// a background receiver. A missing implementation is an error.
	async fn activation_events(
		&self,
	) -> Result<truapi::v01::NotificationActivations, GenericError> {
		Err(GenericError { reason: "notification activation is unsupported".to_string() })
	}

	/// Remove exactly this sequence from the bound activation queue after
	/// successful product routing. Unknown sequences are idempotent; never
	/// acknowledge another product/account/environment or a sequence range.
	async fn acknowledge_activation(
		&self,
		request: truapi::v01::NotificationActivationAcknowledgeRequest,
	) -> Result<(), GenericError> {
		let _ = request;
		Err(GenericError { reason: "notification activation is unsupported".to_string() })
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
#[async_trait::async_trait]
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
		target_product_id: ProductId,
	},
	/// Product-scoped approval to make one kind of signature with another
	/// product's accounts. It is held in memory only and ends when the product
	/// execution closes.
	#[codec(index = 4)]
	CrossProductSignature {
		/// Product whose accounts sign.
		target_product_id: ProductId,
		/// Kind of signature the user approved.
		kind: CrossProductSignatureKind,
	},
	/// Bind and use wallet-held Chat identity authority.
	#[codec(index = 8)]
	ChatAuthority,
	/// Ensure Statement Store quota for this account selector.
	#[codec(index = 9)]
	StatementStoreAllowance {
		/// `None` selects the legacy allowance account.
		derivation_index: Option<DerivationIndex>,
	},
	/// Disclose a profile reference to Chat contacts.
	#[codec(index = 10)]
	ProfileDisclosure,
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
#[async_trait::async_trait]
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

	/// Update a stored permission authorization status. `NotDetermined` resets
	/// the decision to ask again, retaining a tombstone against legacy re-import.
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
		product_id: &ProductId,
		timeout_ms: Option<u32>,
	) -> Result<Option<Bytes32>, GenericError>;
}

/// Pairing-host-only administration API exposed to host UI.
#[async_trait::async_trait]
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
#[async_trait::async_trait]
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

/// A chain the host builds as V5 general transactions, and the transaction
/// extension pipeline it uses there.
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ChainExtensionVersion {
	/// Genesis hash of the chain.
	pub genesis_hash: Bytes32,
	/// The pipeline's number in the chain's metadata.
	pub extension_version: u8,
}

/// The chains a host builds as V5 general transactions. A chain not listed is
/// built as V4.
#[derive(Debug, Clone, Default, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ExtensionVersionMap {
	/// One entry per V5 chain.
	pub chains: Vec<ChainExtensionVersion>,
}

/// Which chains the host builds as V5 general transactions. The core builds
/// its own extrinsics the same way, so both agree on every chain.
#[async_trait::async_trait]
pub trait ExtrinsicVersionHost: Send + Sync {
	/// The chains the host builds as V5, each with its pipeline. A host that
	/// never builds V5 lists none.
	async fn transaction_extension_versions(&self) -> Result<ExtensionVersionMap, GenericError> {
		Ok(ExtensionVersionMap::default())
	}
}

/// Wallet and asset binding checked by the native service before every operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct NativeCoinageScope {
	/// Authenticated root key of the wallet owning the main purse.
	pub root_public_key: [u8; 32],
	/// Genesis hash of the configured Coinage chain.
	pub genesis_hash: [u8; 32],
	/// Configured asset instance; None denotes a legacy single-asset runtime.
	pub coinage_instance_id: Option<u32>,
}

/// Immutable, Host-authenticated outgoing intent. No field is a product display hint.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct NativeCoinagePaymentIntent {
	/// Stable wallet-, network- and product-scoped operation identity.
	pub operation_id: [u8; 32],
	/// Authenticated calling product, not a guest-provided display name.
	pub product_id: String,
	/// Original caller request identifier, bound immutably to this intent.
	pub request_id: String,
	/// Recipient identity authenticated by the Host's Chat authority.
	pub peer_identity: [u8; 32],
	/// Independently resolved recipient name, when available.
	pub recipient_username: Option<String>,
	/// Positive recipient amount in cents of the configured Coinage asset.
	pub amount_cents: u64,
}

/// Host-private bearer material. Never return this through the product API or log it.
/// Raw amounts are canonical unsigned decimal u128 strings, avoiding FFI truncation.
#[derive(Clone, PartialEq, Eq, Encode, Decode, zeroize::Zeroize)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct NativeCoinageMemo {
	/// Validated 64-byte native sr25519 secret keys, confined to the trusted Host.
	pub secret_keys: Vec<Vec<u8>>,
	/// Exact total in canonical decimal raw chain units.
	pub total_value_raw: String,
}

/// Durable native-wallet operations, not an alternative inventory ledger.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum NativeCoinageOperation {
	/// Read trusted denomination metadata without selecting or allocating inventory.
	Denomination,
	/// Native trusted UI must review the exact intent and maximum debit before spending.
	/// Replays must reuse persisted preparation, never allocate a second payment.
	/// Before returning a memo, native custody must survive restart atomically with
	/// transaction registration. Startup must never release these coins as provisional.
	PreparePayment {
		/// Exact request to review or replay from durable native custody.
		intent: NativeCoinagePaymentIntent,
	},
	/// Called only after the authenticated Host durably accepts encrypted delivery.
	/// Records transport acceptance; it is not the first durable native custody mark.
	CommitHandoff {
		/// Product whose ciphertext custody has been committed.
		product_id: String,
		/// Existing native payment being accepted, never a new allocation.
		operation_id: [u8; 32],
	},
	/// Immutable public cards; do not initialize or claim an unrelated wallet.
	Views {
		/// Return public payments only for this authenticated product.
		product_id: String,
	},
	/// Accepted ids come from the Host's durable ciphertext custody ledger.
	PendingHandoffs {
		/// Product whose native payments are being reconciled.
		product_id: String,
		/// Irreversible Host ciphertext custody evidence for acceptance repair.
		accepted_operations: Vec<[u8; 32]>,
	},
	/// Read an existing recoverable handoff without approval, selection or spending.
	ReadHandoff {
		/// Product that originally obtained approval for this payment.
		product_id: String,
		/// Existing payment whose private memo is needed for encrypted replay.
		operation_id: [u8; 32],
	},
	/// Peer acknowledgment is not monetary settlement.
	NoteDelivery {
		/// Product whose authenticated peer acknowledged the payment.
		product_id: String,
		/// Accepted outgoing payment; acknowledgment does not prove settlement.
		operation_id: [u8; 32],
	},
	/// Resume already-owned work only; never authorize a fresh debit.
	Reconcile,
	/// Persist source custody before returning. Only finalized credit is successful.
	/// Reordered sources reuse the same id; changed minimum or overlapping custody conflicts.
	TopUp {
		/// Authenticated product importing these source coins.
		product_id: String,
		/// Stable identity of the canonical source set in this wallet/product.
		operation_id: [u8; 32],
		/// Original immutable minimum in canonical decimal raw chain units.
		/// Zero requests claim-all; it does not waive source completion or finality.
		minimum_amount_raw: String,
		/// Validated native bearer keys to place in durable incoming custody.
		secret_keys: Vec<Vec<u8>>,
	},
}

impl zeroize::Zeroize for NativeCoinageOperation {
	fn zeroize(&mut self) {
		if let Self::TopUp { secret_keys, .. } = self {
			zeroize::Zeroize::zeroize(secret_keys);
		}
	}
}

/// One native operation with the immutable wallet/network scope to authenticate.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct NativeCoinageRequest {
	/// Expected owner and asset, verified against the active native wallet.
	pub scope: NativeCoinageScope,
	/// Host-private command; incoming sources must never be logged.
	pub operation: NativeCoinageOperation,
}

impl zeroize::Zeroize for NativeCoinageRequest {
	fn zeroize(&mut self) {
		zeroize::Zeroize::zeroize(&mut self.operation);
	}
}

/// Sanitized failures. Never forward secret-bearing native exception descriptions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum NativeCoinageFailure {
	/// The selected native owner, its durable store or its session is unavailable.
	Unavailable,
	/// The operation violates amount, identifier or protocol bounds.
	InvalidRequest,
	/// Incoming bearer material is invalid for native Coinage.
	InvalidSource,
	/// An identifier, source set or custody record has conflicting immutable fields.
	OperationConflict,
	/// The payment is absent or belongs to another product.
	OperationNotFound,
	/// Native inventory cannot fund the exact approved debit.
	InsufficientBalance,
	/// Trusted outgoing or privacy review was declined.
	UserRejected,
}

/// Incoming settlement result; acceptance and best-head observations are not finality.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum NativeCoinageTopUpOutcome {
	/// The original requested minimum has been credited at finality.
	/// For a zero minimum, the source claim is terminal with positive finalized credit.
	Cleared,
	/// Terminal shortfall, expressed in raw chain units, not cents.
	Partial {
		/// Positive finalized raw-unit credit, strictly less than the original minimum.
		credited_amount_raw: String,
	},
	/// Durable custody exists, but final credit has not been established.
	Pending,
	/// Terminal: none of the requested funds could be claimed.
	NotClaimed,
}

/// Typed native results. Only the trusted Host may consume a Prepared memo.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum NativeCoinageResponse {
	/// Trusted denomination metadata for the selected wallet/asset.
	Denomination {
		/// Positive raw chain units per Coinage cent, as canonical decimal u128.
		cents_unit_raw: String,
	},
	/// Existing durable native preparation, or a terminal public payment.
	Prepared {
		/// Product-safe payment identity and current state.
		payment: truapi::latest::HostNativeChatPayment,
		/// Recoverable bearer material, absent when no handoff remains necessary.
		memo: Option<NativeCoinageMemo>,
	},
	/// Product-scoped public payment cards, never a spendable balance.
	Payments {
		/// Durable payments belonging to the requested product.
		payments: Vec<truapi::latest::HostNativeChatPayment>,
	},
	/// Result of an incoming custody operation.
	TopUp {
		/// Finalized settlement or explicit retained uncertainty.
		outcome: NativeCoinageTopUpOutcome,
	},
	/// The requested metadata transition or recovery pass completed.
	Done,
	/// Sanitized domain rejection; cannot authorize another backend.
	Failed {
		/// Non-secret failure classification.
		reason: NativeCoinageFailure,
	},
}

impl zeroize::Zeroize for NativeCoinageResponse {
	fn zeroize(&mut self) {
		if let Self::Prepared { memo: Some(memo), .. } = self {
			zeroize::Zeroize::zeroize(memo);
		}
	}
}

/// Optional native wallet service boundary, never exposed to products.
///
/// Injecting this service at runtime construction assigns native custody for
/// the runtime's lifetime. Absence selects the built-in Rust wallet; native
/// unavailability or failure never permits Rust fallback.
#[async_trait::async_trait]
pub trait CoinageWalletHost: Send + Sync {
	/// Invoke the selected native owner. An error never permits Rust fallback.
	async fn native_coinage(
		&self,
		request: NativeCoinageRequest,
	) -> Result<NativeCoinageResponse, GenericError>;
}

/// Trusted native Chat selection context; never passed to a product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct NativeChatFilePickRequest {
	/// Authenticated product requesting selection.
	pub product_id: String,
	/// Host-authenticated recipient identity.
	pub peer_identity: [u8; 32],
	/// Host-resolved recipient name, if available.
	pub peer_username: Option<String>,
	/// Maximum number of files the Host can accept.
	pub max_files: u32,
}

/// Immutable Host-owned source and metadata derived from its actual bytes.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct NativeChatPickedFile {
	/// Opaque private handle surviving restart until explicitly released.
	pub source_id: String,
	/// Actual source size and native media metadata.
	pub metadata: HostNativeChatAttachmentMetadata,
}

/// Trusted context for exporting a verified native Chat attachment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct NativeChatFileExportRequest {
	/// Authenticated product requesting presentation.
	pub product_id: String,
	/// Host-authenticated peer identity.
	pub peer_identity: [u8; 32],
	/// Host-resolved peer name, if available.
	pub peer_username: Option<String>,
	/// Verified attachment metadata, including the exact export size.
	pub metadata: HostNativeChatAttachmentMetadata,
}

/// Host-private native Chat selection, immutable custody and safe export.
///
/// Handles, file bytes and destinations must never be exposed to products.
/// Optional embedders without a backend must fail explicitly as unavailable.
#[async_trait::async_trait]
pub trait NativeChatFilesHost: Send + Sync {
	/// Present trusted selection and durably snapshot the selected files.
	/// An empty result denotes user cancellation, not an unavailable backend.
	async fn pick_chat_files(
		&self,
		request: NativeChatFilePickRequest,
	) -> Result<Vec<NativeChatPickedFile>, GenericError>;

	/// Read exactly `length` bytes from an immutable source. Reject lengths
	/// above 2,000,000 and checked ranges extending beyond its actual u32 size.
	async fn read_chat_file(
		&self,
		source_id: String,
		offset: u64,
		length: u32,
	) -> Result<Vec<u8>, GenericError>;

	/// Release durable source custody. Repeated release is harmless.
	async fn release_chat_file(&self, source_id: String) -> Result<(), GenericError>;

	/// Present trusted export consent and create a private partial output.
	/// `None` denotes user cancellation. Never overwrite without consent.
	async fn begin_chat_file_export(
		&self,
		request: NativeChatFileExportRequest,
	) -> Result<Option<String>, GenericError>;

	/// Append a bounded chunk at the exact current offset; never allow holes,
	/// rewrites, or bytes beyond the declared export size.
	async fn write_chat_file_export(
		&self,
		export_id: String,
		offset: u64,
		data: Vec<u8>,
	) -> Result<(), GenericError>;

	/// Publish only an exact-size completed output through safe native UI.
	/// Never automatically execute HTML, SVG, scripts, or other active files.
	async fn finish_chat_file_export(&self, export_id: String) -> Result<(), GenericError>;

	/// Idempotently discard a partial output, never a completed user export.
	async fn cancel_chat_file_export(&self, export_id: String) -> Result<(), GenericError>;
}

/// Host-private HOP transport for the configured Bulletin chain.
///
/// Endpoints are trusted host configuration, never product-supplied dialing
/// instructions. Implementations must re-read their allowlist and enforce exact
/// URL membership before opening a connection. An unconfigured host is
/// explicitly unavailable; it must not fall back to a chain RPC endpoint.
#[async_trait::async_trait]
pub trait HopProvider: Send + Sync {
	/// Current exact WSS endpoints from the host's trusted Bulletin registry.
	async fn allowed_hop_endpoints(
		&self,
		bulletin_genesis_hash: [u8; 32],
	) -> Result<Vec<String>, GenericError> {
		let _ = bulletin_genesis_hash;
		Ok(Vec::new())
	}

	/// Open one private HOP connection, sharing the JSON-RPC lifecycle only.
	/// The caller closes the returned lease when the private operation ends.
	async fn connect_hop(
		&self,
		bulletin_genesis_hash: [u8; 32],
		endpoint: String,
	) -> Result<Box<dyn JsonRpcConnection>, GenericError> {
		let _ = (bulletin_genesis_hash, endpoint);
		Err(GenericError { reason: "HOP provider unavailable".to_string() })
	}
}

/// Check an endpoint without normalizing it into a different allowlist entry.
pub fn ensure_allowed_hop_endpoint(endpoint: &str, allowed: &[String]) -> Result<(), GenericError> {
	let valid = endpoint.starts_with("wss://") &&
		!endpoint.chars().any(|ch| ch.is_whitespace() || ch.is_control()) &&
		!endpoint.contains(['#', '\\']) &&
		!endpoint[6..].split(['/', '?']).next().unwrap_or_default().contains('@') &&
		url::Url::parse(endpoint).is_ok_and(|url| {
			url.scheme() == "wss" &&
				url.host_str().is_some() &&
				url.username().is_empty() &&
				url.password().is_none() &&
				url.fragment().is_none()
		}) && allowed.iter().any(|entry| entry == endpoint);
	if !valid {
		return Err(GenericError {
			reason: "HOP endpoint is not in the trusted Bulletin WSS allowlist".to_string(),
		});
	}
	Ok(())
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
	// A new outer namespace prevents every legacy authorization encoding from
	// being read as a current grant. Migration archives old decisions inertly.
	#[codec(index = 128)]
	PermissionAuthorization {
		/// Stored full identifier or bare manifest label.
		scope: String,
		/// Permission encoded under this scope.
		request: StoredPermissionRequest,
	},
	/// Persisted allowance-slot keys for one paired SSO session.
	#[codec(index = 3)]
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
		product_id: ProductId,
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
		product_id: ProductId,
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
		/// Bare product label whose manifest was cached.
		product_label: String,
	},
	/// A product's Worker executable manifest as last read from dotNS, and when,
	/// cached for the same lifetime as [`Self::ProductManifest`].
	#[codec(index = 14)]
	WorkerManifest {
		/// Bare label of the product whose Worker manifest was cached.
		product_label: String,
	},
	/// What funding providers' quote answers showed about what they serve,
	/// each record trusted for twelve hours.
	#[codec(index = 15)]
	FundingSupport,
	/// Wallet-owned encrypted inventory, operation journal and claim recovery.
	#[codec(index = 13)]
	MainPurseCoinage {
		/// Wallet root.
		root_public_key: [u8; 32],
		/// Coinage network.
		genesis_hash: [u8; 32],
	},
	/// Wallet-owned encrypted native Chat device and migration history.
	#[codec(index = 64)]
	NativeChatDevice {
		/// Wallet root.
		root_public_key: [u8; 32],
		/// Chat network.
		genesis_hash: [u8; 32],
		/// Canonical authenticated product; persisted string also binds value encryption.
		product_id: String,
	},
	/// Bounded encrypted attachment chunk, indexed by its owning product.
	#[codec(index = 65)]
	NativeChatFileChunk {
		/// Wallet root.
		root_public_key: [u8; 32],
		/// Chat network.
		genesis_hash: [u8; 32],
		/// Canonical authenticated product.
		product_id: String,
		/// Opaque public attachment identifier.
		attachment_id: [u8; 32],
		/// Exact chunk index.
		chunk_index: u32,
	},
	/// Wallet-owned installed Chat products; restoration rechecks current grants.
	#[codec(index = 16)]
	NativeChatProducts {
		/// Wallet root.
		root_public_key: [u8; 32],
		/// Chat network.
		genesis_hash: [u8; 32],
	},
	/// Wallet-owned profile reference disclosed on a Chat network.
	#[codec(index = 17)]
	ProfileDisclosure {
		/// Wallet root.
		root_public_key: [u8; 32],
		/// Chat network.
		genesis_hash: [u8; 32],
	},
	/// Bearer profile references received by one product's contacts.
	#[codec(index = 18)]
	ProfileReferencesReceived {
		/// Wallet root.
		root_public_key: [u8; 32],
		/// Chat network.
		genesis_hash: [u8; 32],
		/// Canonical authenticated product.
		product_id: String,
	},
	/// Wallet-wide personal profile grants and replay tombstones.
	#[codec(index = 19)]
	ProfilePersonalReferencesReceived {
		/// Wallet root.
		root_public_key: [u8; 32],
		/// Chat network.
		genesis_hash: [u8; 32],
	},
	/// Bounded receiving ledger; revoke receiving before product/account removal.
	#[codec(index = 20)]
	NotificationReceiving,
	/// Historical funding sessions, preserved for migration and recovery.
	#[codec(index = 21)]
	FundingSessions,
	/// Unreadable historical funding blobs, retained rather than overwritten.
	#[codec(index = 22)]
	FundingSessionsUnreadable,
	/// Historical compacted funding sessions; never trimmed.
	#[codec(index = 23)]
	FundingSessionPack,
}

/// Permission storage payload, whose targets can be bare manifest labels.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum StoredPermissionRequest {
	/// Device permission decision.
	Device(HostDevicePermissionRequest),
	/// Remote permission decision.
	Remote(RemotePermissionRequest),
	/// Identity disclosure decision.
	IdentityDisclosure,
	/// Account access decision.
	AccountAccess {
		/// Full identifier or bare label in the stored scope.
		target: String,
	},
	/// Execution-local approval for one signature kind.
	CrossProductSignature {
		/// Bare label whose accounts sign.
		target: String,
		/// Kind of signature approved.
		kind: CrossProductSignatureKind,
	},
	/// Current released plus experimental remote permission schema.
	#[codec(index = 111)]
	RemoteV2(RemotePermissionRequest),
	/// Current Chat authority decision.
	#[codec(index = 8)]
	ChatAuthority,
	/// Current Statement Store allowance decision.
	#[codec(index = 9)]
	StatementStoreAllowance {
		/// Account selector.
		derivation_index: Option<DerivationIndex>,
	},
	/// Current profile disclosure decision.
	#[codec(index = 10)]
	ProfileDisclosure,
}

impl From<PermissionAuthorizationRequest> for StoredPermissionRequest {
	fn from(request: PermissionAuthorizationRequest) -> Self {
		match request {
			PermissionAuthorizationRequest::Device(permission) => Self::Device(permission),
			PermissionAuthorizationRequest::Remote(request) => Self::RemoteV2(request),
			PermissionAuthorizationRequest::IdentityDisclosure => Self::IdentityDisclosure,
			PermissionAuthorizationRequest::AccountAccess { target_product_id } => {
				Self::AccountAccess { target: target_product_id.into() }
			},
			PermissionAuthorizationRequest::CrossProductSignature { target_product_id, kind } => {
				Self::CrossProductSignature { target: target_product_id.into(), kind }
			},
			PermissionAuthorizationRequest::ChatAuthority => Self::ChatAuthority,
			PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index } => {
				Self::StatementStoreAllowance { derivation_index }
			},
			PermissionAuthorizationRequest::ProfileDisclosure => Self::ProfileDisclosure,
		}
	}
}

impl StoredPermissionRequest {
	/// Reconstruct a settings request in the querying product's network.
	///
	/// Shared account targets are stored as bare labels. They may only be
	/// expanded using the caller's explicit network, never a default network.
	/// Retired remote encodings cannot become current authorization requests.
	pub fn authorization_request(
		&self,
		product: &ProductId,
	) -> Option<PermissionAuthorizationRequest> {
		let target_product = |target: &str| {
			target.parse::<ProductId>().ok().or_else(|| {
				if !is_dns_label(target) {
					return None;
				}
				ProductId::from_name(target, product.network_suffix()?).ok()
			})
		};
		Some(match self {
			Self::Device(permission) => PermissionAuthorizationRequest::Device(*permission),
			Self::Remote(_) => return None,
			Self::RemoteV2(request) => PermissionAuthorizationRequest::Remote(request.clone()),
			Self::IdentityDisclosure => PermissionAuthorizationRequest::IdentityDisclosure,
			Self::AccountAccess { target } => PermissionAuthorizationRequest::AccountAccess {
				target_product_id: target_product(target)?,
			},
			Self::CrossProductSignature { target, kind } => {
				PermissionAuthorizationRequest::CrossProductSignature {
					target_product_id: target_product(target)?,
					kind: *kind,
				}
			},
			Self::ChatAuthority => PermissionAuthorizationRequest::ChatAuthority,
			Self::StatementStoreAllowance { derivation_index } => {
				PermissionAuthorizationRequest::StatementStoreAllowance {
					derivation_index: derivation_index.clone(),
				}
			},
			Self::ProfileDisclosure => PermissionAuthorizationRequest::ProfileDisclosure,
		})
	}
}

/// Stable metadata describing one strictly decoded [`CoreStorageKey`].
///
/// `kind` is the Rust variant name and is part of the host embedding contract.
/// `owner` distinguishes full product identities from shared manifest labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreStorageKeyDescription {
	/// Stable storage-key variant name.
	pub kind: &'static str,
	/// Product or manifest label that owns this slot.
	pub owner: Option<CoreStorageKeyOwner>,
}

/// The identity used to scope a persisted slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreStorageKeyOwner {
	/// An exact product executable.
	Product(ProductId),
	/// A manifest shared by the product's executable subnames.
	ProductLabel(String),
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
	let (kind, owner) = match key {
		CoreStorageKey::AuthSession => ("AuthSession", None),
		CoreStorageKey::PairingDeviceIdentity => ("PairingDeviceIdentity", None),
		CoreStorageKey::PermissionAuthorization { scope, .. } => {
			let owner = match scope.parse() {
				Ok(product_id) => CoreStorageKeyOwner::Product(product_id),
				Err(_) if is_dns_label(&scope) => CoreStorageKeyOwner::ProductLabel(scope),
				Err(_) => return Err(CoreStorageKeyDescriptionError::InvalidEncoding),
			};
			("PermissionAuthorization", Some(owner))
		},
		CoreStorageKey::AllowanceKeys { .. } => ("AllowanceKeys", None),
		CoreStorageKey::LastProcessedPairingStatement => ("LastProcessedPairingStatement", None),
		CoreStorageKey::AutoSigningKey { product_id } => {
			("AutoSigningKey", Some(CoreStorageKeyOwner::Product(product_id)))
		},
		CoreStorageKey::ProductSubtree { product_id, .. } => {
			("ProductSubtree", Some(CoreStorageKeyOwner::Product(product_id)))
		},
		CoreStorageKey::AutoSigningKeys => ("AutoSigningKeys", None),
		CoreStorageKey::RingVrfRegistry { .. } => ("RingVrfRegistry", None),
		CoreStorageKey::StatementRenewalTargets => ("StatementRenewalTargets", None),
		CoreStorageKey::DeviceEncryptionKey => ("DeviceEncryptionKey", None),
		CoreStorageKey::SsoResponderRequestLedger { .. } => ("SsoResponderRequestLedger", None),
		CoreStorageKey::ProductManifest { product_label } => {
			("ProductManifest", Some(CoreStorageKeyOwner::ProductLabel(product_label)))
		},
		CoreStorageKey::WorkerManifest { product_label } => {
			("WorkerManifest", Some(CoreStorageKeyOwner::ProductLabel(product_label)))
		},
		CoreStorageKey::FundingSupport => ("FundingSupport", None),
		CoreStorageKey::MainPurseCoinage { .. } => ("MainPurseCoinage", None),
		CoreStorageKey::NativeChatDevice { .. } => ("NativeChatDevice", None),
		CoreStorageKey::NativeChatProducts { .. } => ("NativeChatProducts", None),
		CoreStorageKey::ProfileDisclosure { .. } => ("ProfileDisclosure", None),
		CoreStorageKey::ProfileReferencesReceived { product_id, .. } => (
			"ProfileReferencesReceived",
			Some(CoreStorageKeyOwner::Product(
				product_id
					.parse()
					.map_err(|_| CoreStorageKeyDescriptionError::InvalidEncoding)?,
			)),
		),
		CoreStorageKey::ProfilePersonalReferencesReceived { .. } => {
			("ProfilePersonalReferencesReceived", None)
		},
		CoreStorageKey::NativeChatFileChunk { product_id, .. } => (
			"NativeChatFileChunk",
			Some(CoreStorageKeyOwner::Product(
				product_id
					.parse()
					.map_err(|_| CoreStorageKeyDescriptionError::InvalidEncoding)?,
			)),
		),
		CoreStorageKey::NotificationReceiving => ("NotificationReceiving", None),
		CoreStorageKey::FundingSessions => ("FundingSessions", None),
		CoreStorageKey::FundingSessionsUnreadable => ("FundingSessionsUnreadable", None),
		CoreStorageKey::FundingSessionPack => ("FundingSessionPack", None),
	};
	Ok(CoreStorageKeyDescription { kind, owner })
}

impl CoreStorageKey {
	/// Cache key for `product_id`'s root manifest.
	///
	/// Keyed by [`ProductId::bare_label`], which is what the lookup resolves on
	/// the host's network, so `peopl.dot` and `app.peopl.dot` share one entry.
	/// Keying by the full id would let a caller force a chain lookup and a
	/// storage write for every subname it makes up. Code that seeds this cache
	/// must use this key.
	pub fn product_manifest(product_id: &ProductId) -> Self {
		Self::ProductManifest { product_label: product_id.bare_label().to_string() }
	}

	/// Persist an authorization under the full product identity.
	pub fn permission_authorization(
		product_id: &ProductId,
		request: PermissionAuthorizationRequest,
	) -> Self {
		Self::PermissionAuthorization { scope: product_id.to_string(), request: request.into() }
	}

	/// Persisted authorization key for one product-scoped device permission.
	pub fn device_permission_authorization(
		product_id: &ProductId,
		permission: &HostDevicePermissionRequest,
	) -> Self {
		Self::permission_authorization(
			product_id,
			PermissionAuthorizationRequest::Device(*permission),
		)
	}

	/// Persisted authorization key for one product-scoped remote permission.
	pub fn remote_permission_authorization(
		product_id: &ProductId,
		request: &RemotePermissionRequest,
	) -> Self {
		Self::permission_authorization(
			product_id,
			PermissionAuthorizationRequest::Remote(canonical_remote_request(request)),
		)
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
	pub fn remote_domain_authorization(product_id: &ProductId, domain: &str) -> Self {
		Self::remote_permission_authorization(
			product_id,
			&RemotePermissionRequest {
				permission: RemotePermission::Remote { domains: vec![domain.to_string()] },
			},
		)
	}

	/// Persisted authorization key for product-scoped identity disclosure.
	pub fn identity_disclosure_authorization(product_id: &ProductId) -> Self {
		Self::permission_authorization(
			product_id,
			PermissionAuthorizationRequest::IdentityDisclosure,
		)
	}

	/// Persist an account decision under exact executable identities.
	pub fn account_access_authorization(
		product_id: &ProductId,
		target_product_id: &ProductId,
	) -> Self {
		Self::permission_authorization(
			product_id,
			PermissionAuthorizationRequest::AccountAccess {
				target_product_id: target_product_id.clone(),
			},
		)
	}

	/// Persist an account decision shared by every executable of both products.
	pub fn product_account_access_authorization(
		product_id: &ProductId,
		target_product_id: &ProductId,
	) -> Self {
		Self::PermissionAuthorization {
			scope: product_id.bare_label().to_string(),
			request: StoredPermissionRequest::AccountAccess {
				target: target_product_id.bare_label().to_string(),
			},
		}
	}

	/// Signature approval shared by the product's executable subnames.
	pub fn product_signature_authorization(
		product_id: &ProductId,
		target_product_id: &ProductId,
		kind: CrossProductSignatureKind,
	) -> Self {
		Self::PermissionAuthorization {
			scope: product_id.bare_label().to_string(),
			request: StoredPermissionRequest::CrossProductSignature {
				target: target_product_id.bare_label().to_string(),
				kind,
			},
		}
	}

	/// Fresh product-scoped Chat authority decision.
	pub fn chat_authority_authorization(product_id: &ProductId) -> Self {
		Self::permission_authorization(product_id, PermissionAuthorizationRequest::ChatAuthority)
	}

	/// Fresh product-scoped statement allowance decision.
	pub fn statement_store_allowance_authorization(
		product_id: &ProductId,
		derivation_index: Option<DerivationIndex>,
	) -> Self {
		Self::permission_authorization(
			product_id,
			PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index },
		)
	}

	/// Fresh product-scoped profile disclosure decision.
	pub fn profile_disclosure_authorization(product_id: &ProductId) -> Self {
		Self::permission_authorization(
			product_id,
			PermissionAuthorizationRequest::ProfileDisclosure,
		)
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
mod tests;

/// Host-private persistence for core-owned state.
///
/// Clearing product-indexed slots is the host's job. The core drops the ones
/// it is holding when a session ends, but a product it never opened this run
/// has no entry to drop, so those slots outlive the disconnect. A host that
/// removes a product must clear them with the rest of that product's state, or
/// they accumulate for the life of the install.
///
/// [`describe_core_storage_key`] names the product owning a slot:
/// [`CoreStorageKeyDescription::owner`] names a full product for the
/// product-indexed variants, which are `PermissionAuthorization`,
/// `AutoSigningKey`, and `ProductSubtree`. Keying host storage by that value
/// makes the sweep a prefix delete rather than a scan.
// TODO: introduce SecretStorage to make protection a separate contract.
#[async_trait::async_trait]
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
	Connected(Box<SessionUiInfo>),
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
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
		calling_product_id: Option<ProductId>,
		/// Signing request.
		request: HostSignPayloadRequest,
	},
	/// Legacy-account signing request, which only the product's account at
	/// index 0 may sign.
	LegacyAccount {
		/// Account that will sign.
		account: ProductAccountId,
		/// Signing request.
		request: HostSignPayloadWithLegacyAccountRequest,
	},
}

/// Account a legacy-account request resolved to, so a host knows the signer
/// without resolving the request's address itself.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum LegacySigningAccount {
	/// The wallet's identity account.
	Identity,
	/// The requesting product's account at derivation index 0.
	Product(ProductAccountId),
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
		calling_product_id: Option<ProductId>,
		/// Raw signing request.
		request: HostSignRawRequest,
		/// Whether the signer applies the `<Bytes>` transaction-payload protection.
		watermarked: bool,
	},
	/// Legacy-account raw signing request.
	LegacyAccount {
		/// Account that will sign.
		account: LegacySigningAccount,
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
	pub calling_product_id: Option<ProductId>,
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
		calling_product_id: Option<ProductId>,
		/// Transaction request.
		payload: ProductAccountTxPayload,
	},
	/// Legacy-account transaction request.
	LegacyAccount {
		/// Account that will sign.
		account: LegacySigningAccount,
		/// Transaction request.
		payload: LegacyAccountTxPayload,
	},
}

/// Review shown before a product derives a contextual alias (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct AccountAliasReview {
	/// Product requesting the alias.
	pub calling_product_id: ProductId,
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
	pub calling_product_id: ProductId,
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
	pub calling_product_id: ProductId,
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
	pub calling_product_id: ProductId,
	/// Resources to allocate.
	pub resources: Vec<AllocatableResource>,
}

/// Review shown before a product asks to access another product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct AccountAccessReview {
	/// Product currently handling the request.
	pub requesting_product_id: ProductId,
	/// Product whose account is being requested.
	pub target_product_id: ProductId,
}

/// Review shown before a product learns the user's primary identity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct IdentityDisclosureReview {
	/// Product currently handling the request.
	pub product_id: ProductId,
}

/// Review shown before a product binds or uses wallet-held Chat identity authority.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ChatAuthorityReview {
	/// Product requesting the Chat identity operation.
	pub product_id: String,
}

/// Review shown before a product discloses a profile reference to an app
/// audience or selected contacts. Personal grants permit host rendering across
/// recipient apps. This authorizes the product, not individual audience edits.
/// The prompt names the product, never the contacts or the reference.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct ProfileDisclosureReview {
	/// Product asking to disclose the profile.
	pub product_id: String,
}

/// Exact Host-resolved payment reviewed before debiting the user's main purse.
///
/// This review never grants a reusable spending permission. Chat authority and
/// automatic product signing do not authorize it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct MainPurseChatPaymentReview {
	/// Authenticated product requesting this payment.
	pub calling_product_id: String,
	/// Recipient identity authenticated by the Host's native Chat session.
	pub recipient_identity: [u8; 32],
	/// Host-resolved username for that identity, never a product display label.
	pub recipient_username: Option<String>,
	/// Exact recipient amount in cents of the selected Coinage asset.
	pub amount_cents: u64,
	/// Maximum main-purse debit, including any approved fee, in the same cents.
	pub max_debit_cents: u64,
	/// Genesis hash of the Host-selected Coinage chain.
	pub genesis_hash: [u8; 32],
	/// Trusted Coinage asset instance; None denotes a legacy single-asset runtime.
	pub coinage_instance_id: Option<u32>,
	/// Immutable, wallet-scoped payment operation being authorized.
	pub operation_id: [u8; 32],
}

/// Review shown before a product resolves its own account subtree over SSO,
/// when the value is not cached and the core must ask the Account Holder.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ProductSubtreeReview {
	/// Product resolving its own account.
	pub product_id: ProductId,
}

/// Review shown before a product moves items out of an NFT purse it does
/// not own: Root's, or another product's.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct NftPurseDrainReview {
	/// Product making the request.
	pub calling_product_id: ProductId,
	/// Purse the items leave.
	pub from_purse: u32,
	/// Who owns that purse.
	pub owner: NftPurseOwner,
	/// How many items leave it.
	pub item_count: u32,
	/// Where they go.
	pub destination: NftPurseDrainDestination,
}

/// Where items drained from a purse go.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum NftPurseDrainDestination {
	/// Fresh keys of this purse.
	Purse {
		/// The purse's id.
		purse: u32,
	},
	/// This key, outside every purse.
	ExternalKey {
		/// The key's public bytes.
		key: Bytes32,
	},
	/// A cheque the product delivers to its recipient.
	Cheque,
	/// The keys of a deposited cheque, back to its sender.
	Refund,
}

/// Review shown before a user-confirmed core action continues.
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
	// Index 8 remains unused to prevent legacy review reinterpretation.
	/// Allow a product to access another product account.
	#[codec(index = 9)]
	AccountAccess(AccountAccessReview),
	/// Sign an RFC-0023 VRF transcript with a product account.
	#[codec(index = 10)]
	SignVrf(SignVrfReview),
	/// Resolve a product's own account subtree over SSO.
	#[codec(index = 11)]
	ProductSubtree(ProductSubtreeReview),
	/// Move items out of an NFT purse the product does not own.
	#[codec(index = 12)]
	NftPurseDrain(NftPurseDrainReview),
	/// Bind and use wallet-held Chat identity authority.
	#[codec(index = 32)]
	ChatAuthority(ChatAuthorityReview),
	/// Confirm one exact main-purse payment; never automatically approved.
	#[codec(index = 33)]
	MainPurseChatPayment(MainPurseChatPaymentReview),
	/// Disclose a profile reference to Chat contacts.
	#[codec(index = 34)]
	ProfileDisclosure(ProfileDisclosureReview),
}

/// Local user confirmation UI for sensitive core-owned operations.
#[async_trait::async_trait]
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
#[async_trait::async_trait]
pub trait LocaleHost: Send + Sync {
	/// Emits the currently selected locale immediately, then future changes.
	fn subscribe_locale(&self)
	-> BoxStream<'static, Result<HostLocaleSubscribeItem, GenericError>>;

	/// Convert a bounded UTC batch using the supplied host locale snapshot.
	async fn localize_timestamps(
		&self,
		_request: crate::latest::HostLocaleLocalizeTimestampsRequest,
	) -> Result<crate::latest::HostLocaleLocalizeTimestampsResponse, GenericError> {
		Err(GenericError { reason: "Local time conversion is unavailable".into() })
	}
}

/// Optional host-authenticated username candidate source. The host owns backend
/// configuration and credentials. Candidates are never ownership assertions:
/// the core checks finalized dotNS ownership and the canonical People Chat key.
#[async_trait::async_trait]
pub trait IdentityBackendHost: Send + Sync {
	/// Return exact-name candidates on the specified People network. Reject
	/// unavailable configuration/authentication and incomplete search results.
	/// Neither a guest URL nor a backend display label crosses this boundary.
	async fn identity_username_candidates(
		&self,
		username: String,
		people_chain_genesis_hash: [u8; 32],
	) -> Result<Vec<[u8; 32]>, GenericError>;
}

/// Host preimage backend. The core builds, signs, and submits the Bulletin
/// `TransactionStorage.store` transaction itself; the host only owns preimage
/// content retrieval (P2P/IPFS lookup).
#[async_trait::async_trait]
pub trait PreimageHost: Send + Sync {
	/// Emits current value/miss immediately, then future updates. `key` is
	/// the blake2b-256 digest of the preimage.
	fn lookup_preimage(
		&self,
		key: [u8; 32],
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
#[async_trait::async_trait]
pub trait ChatPlatform: Send + Sync {
	/// Create or resolve a product-scoped native chat room.
	async fn create_chat_room(
		&self,
		product: &ProductContext,
		request: HostChatCreateRoomRequest,
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
#[async_trait::async_trait]
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
#[async_trait::async_trait]
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

/// The providers' quotes for one ask, as the host draws its provider list.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingQuoteList {
	/// The request this list answers, as `request_funding_quotes` returned
	/// it. Ids only grow, and a list can reach the host before the call that
	/// returns its id does, so a host shows a list only when its id is at
	/// least the latest it has seen, and drops the rest.
	pub request_id: u64,
	/// One row per provider asked, in the host's provider order.
	pub rows: Vec<FundingQuoteRow>,
	/// The provider whose quote is best for the user, which the host selects
	/// until the user picks another: the least to pay inbound, the most to
	/// receive outbound. `None` until one quotes.
	pub best: Option<ProductId>,
	/// When the core asks every provider again, in Unix milliseconds. Each
	/// row keeps its answer until the provider answers again.
	pub refresh_at_ms: u64,
}

/// One provider's place in the list a quote request fills in.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct FundingQuoteRow {
	/// The provider.
	pub provider_id: ProductId,
	/// Where its quote stands.
	pub state: FundingQuoteState,
}

/// Where a provider's quote stands.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingQuoteState {
	/// Asked, not answered yet.
	Pending,
	/// The provider's price.
	Quoted {
		/// The quote.
		quote: FundingQuote,
	},
	/// The provider cannot be offered for this ask; the host shows it
	/// disabled.
	Unavailable {
		/// Why.
		reason: FundingQuoteUnavailable,
	},
}

/// Why a provider cannot be offered for an ask.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Enum))]
pub enum FundingQuoteUnavailable {
	/// The provider refused to price it.
	Refused {
		/// Why.
		reason: FundingQuoteRefusal,
	},
	/// The provider did not answer in time.
	Timeout,
}

/// How the user left the funding overlay.
#[derive(Debug, Clone, Copy)]
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
#[async_trait::async_trait]
pub trait FundingPlatform: Send + Sync {
	/// Show the funding overlay for a session that `product` opened, or the
	/// host itself when `product` is `None`.
	async fn present_funding(
		&self,
		product: Option<&ProductContext>,
		session: FundingPresentation,
	) -> Result<FundingPresentOutcome, GenericError>;

	/// Show `target` for `provider`'s session `intent` in a frame the host
	/// owns, and answer once it closes. A page is an https URL the core has
	/// checked: the host shows its origin, and closes the frame with
	/// `Returned` when the page reaches its return URL.
	async fn present_provider_frame(
		&self,
		provider: &ProductContext,
		intent: String,
		target: truapi::latest::FundingFrameTarget,
	) -> Result<truapi::latest::FundingFrameOutcome, GenericError>;

	/// Close the frame `present_provider_frame` shows for `provider`'s
	/// session `intent`, answering that call `Closed`, and show the
	/// session's progress in its place. Does nothing when no frame is
	/// open for it.
	fn close_provider_frame(&self, provider: &ProductContext, intent: String);

	/// Whether `provider` ships an App, so an App route of its can be shown.
	/// A provider without one shows only external pages.
	async fn provider_has_app(&self, _provider: &ProductContext) -> bool {
		true
	}

	/// The country the user is likely in, as an ISO 3166-1 alpha-2 code, for
	/// the quotes the core asks before the user picks one. Best effort,
	/// without asking for any permission: the device's coarse location when
	/// already permitted, then its SIM or network country, then its locale's
	/// region. `None` when nothing says.
	async fn detected_country(&self) -> Option<String> {
		None
	}

	/// Observe a session's status change, for host UI such as the in-flight
	/// pill.
	fn funding_session_changed(&self, intent: String, status: HostFundingStatusSubscribeItem) {
		let _ = (intent, status);
	}

	/// Observe the quote list the host requested, for the provider list:
	/// again whenever a provider answers, and each time the core asks every
	/// provider again.
	fn funding_quotes_changed(&self, intent: String, quotes: FundingQuoteList) {
		let _ = (intent, quotes);
	}
}

/// Adapter through which product Pocket calls reach the card collection.
/// Optional: a host that omits it leaves Pocket requests answered
/// `Unsupported`. See [`OptionalPlatform`]. Native hosts are served by the
/// core's own collection in its database.
#[async_trait::async_trait]
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

/// Host-implemented adapter that shows a product-referenced profile in
/// host-owned UI. Optional: a host that omits it leaves Profile requests
/// answered `Unsupported`. See [`OptionalPlatform`].
///
/// The reference is a bearer capability. The host resolves, decrypts and
/// renders it; profile bytes and the reference's key never return to the
/// product. The core screens only the reference's shape, so parsing it and
/// deciding what it may fetch are the host's.
#[async_trait::async_trait]
pub trait ProfilePlatform: Send + Sync {
	/// Take one presentation and return once it is shown, never waiting for
	/// the user to dismiss it. Report an unparseable reference as
	/// `InvalidReference`; show load and fetch failures in the UI instead.
	async fn present_profile(
		&self,
		product: &ProductContext,
		request: HostProfilePresentRequest,
	) -> Result<(), HostProfilePresentError>;

	/// Show a Chat contact's shared profile, or host-owned feedback when no
	/// profile is shared. Return once it is shown, without waiting for dismissal.
	/// Report an unparseable shared reference as `InvalidReference`.
	///
	/// The core holds this reference because it arrived over the
	/// authenticated Chat channel from `peer_identity`'s own device, so the
	/// host can name that contact as who shared it, rather than the product
	/// that asked. It cannot vouch for more: the record behind the reference
	/// is not signed by its owner, so a contact can forward someone else's
	/// reference. The default presents a shared profile as
	/// [`ProfilePlatform::present_profile`] would, without the contact, and
	/// reports an error when empty-profile feedback is unsupported.
	async fn present_contact_profile(
		&self,
		product: &ProductContext,
		presented: PresentedContactProfile,
	) -> Result<(), HostProfilePresentError> {
		let shared = presented.shared.ok_or_else(|| HostProfilePresentError::Unknown {
			reason: "Contact profile feedback is unavailable".to_string(),
		})?;
		self.present_profile(product, HostProfilePresentRequest { reference: shared.reference })
			.await
	}

	/// Draw the contact avatars a product placed, on the host's own layer over
	/// the product's surface, replacing what was drawn for it before; an empty
	/// `avatars` clears it. The layer must let pointer input through to the
	/// product and must never tell the product what it drew.
	///
	/// The core calls this again, with the product's last geometry, whenever
	/// a contact on it shares, re-shares or withdraws a profile, and with no
	/// avatars once
	/// the product's connection goes away. Answer `Unsupported` if this host
	/// cannot draw over the product; the product is told so. The default draws
	/// nothing.
	async fn place_contact_avatars(
		&self,
		product: &ProductContext,
		placed: PlacedAvatars,
	) -> Result<(), HostProfilePlaceContactAvatarsError> {
		let _ = (product, placed);
		Ok(())
	}
}

/// Host-only presentation of a contact's shared profile or its absence.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct PresentedContactProfile {
	/// The profile currently shared with the user. `None` means no received,
	/// unretracted profile, never a storage or loading failure.
	pub shared: Option<SharedContactProfile>,
	/// The contact being presented. When shared, their authenticated Chat
	/// device delivered the reference, not necessarily their own profile.
	pub peer_identity: [u8; 32],
	/// The contact's username, when the core knows one: the name its Chat
	/// roster holds for `peer_identity`, verified when the contact was bound
	/// or first authenticated, else the peer's verified dotNS name. Never a
	/// name from the product. `None` when neither is known in time; show the
	/// contact without a name then, never by address.
	pub username: Option<String>,
}

/// A profile reference received from an authenticated Chat contact.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct SharedContactProfile {
	/// The profile reference the contact disclosed. A bearer capability, as
	/// in [`ProfilePlatform::present_profile`].
	pub reference: String,
	/// The share's freshness timestamp, as in [`PlacedAvatar::shared_at`].
	/// Personal grants advance it monotonically across relay actors.
	pub shared_at: u64,
}

impl core::fmt::Debug for SharedContactProfile {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("SharedContactProfile")
			.field("reference", &"[REDACTED]")
			.field("shared_at", &self.shared_at)
			.finish()
	}
}

/// The avatars the core found drawable in one product's placement: the slots
/// whose contact shared a profile with the user, each with the reference that
/// contact disclosed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct PlacedAvatars {
	/// Width of the product's surface, in the units of every rect.
	pub surface_width: u32,
	/// Height of the product's surface, in the same units.
	pub surface_height: u32,
	/// Avatars to draw, in the product's slot order.
	pub avatars: Vec<PlacedAvatar>,
}

/// One avatar to draw over a product.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(all(feature = "runtime", not(target_arch = "wasm32")), derive(uniffi::Record))]
pub struct PlacedAvatar {
	/// The product's id for this on-screen avatar, stable across updates.
	pub slot: u32,
	/// Bounding box of the avatar circle, in surface units.
	pub rect: AvatarRect,
	/// Visible region the avatar is cut to, in surface units.
	pub clip: AvatarRect,
	/// The profile reference the contact disclosed. A bearer capability, as
	/// in [`ProfilePlatform::present_profile`].
	pub reference: String,
	/// Freshness token for this reference. Contact shares use Unix
	/// milliseconds, advanced monotonically for personal revisions even
	/// across relay actors with different clocks. The own avatar uses the
	/// disclosure revision. A changed token invalidates cached contents;
	/// do not interpret an own-profile token as a wall-clock date.
	pub shared_at: u64,
}

impl core::fmt::Debug for PlacedAvatar {
	fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		f.debug_struct("PlacedAvatar")
			.field("slot", &self.slot)
			.field("rect", &self.rect)
			.field("clip", &self.clip)
			.field("reference", &"[REDACTED]")
			.field("shared_at", &self.shared_at)
			.finish()
	}
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
#[async_trait::async_trait]
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
#[async_trait::async_trait]
pub trait PermissionStatusHost: Send + Sync {
	/// Current OS status of a device capability. Must not prompt.
	async fn device_permission_status(
		&self,
		request: HostDevicePermissionRequest,
	) -> Result<DevicePermissionStatus, GenericError>;
}

/// What the host did with a request to show or hide the expanded card face.
#[derive(Debug, Clone, Encode, Decode)]
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
#[async_trait::async_trait]
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
#[async_trait::async_trait]
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
#[derive(Clone, Encode, Decode)]
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
#[derive(Debug, Clone, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct HostContactMatches {
	/// One entry per requested handle, in order: the contact's account, or
	/// `None` when no current contact hashes to it.
	pub accounts: Vec<Option<Bytes32>>,
}

/// How a host's contact picker ended.
#[derive(Debug, Clone, Copy, Encode, Decode)]
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

/// Host-private initial selection for a multi-contact picker.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct ContactSelection {
	/// Resolved accounts to preselect, deduplicated and bounded to 256.
	pub selected: Vec<Bytes32>,
}

/// The user's complete selection in a host-owned multi-contact picker.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Enum))]
pub enum HostContactsPick {
	/// Confirmed accounts, including an empty selection. Never sent to products.
	Picked {
		/// Chosen contact accounts, at most 256.
		accounts: Vec<Bytes32>,
	},
	/// The user cancelled without changing the selection.
	Dismissed,
	/// There are no contacts to show.
	NoContacts,
	/// This host cannot present a multi-contact picker.
	Unsupported,
}

/// One contact name to render in host-owned UI, without any Profile grant.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PlacedContactLabel {
	/// Stable, product-chosen placement id.
	pub slot: u32,
	/// Resolved contact account, never sent to the product.
	pub account: Bytes32,
	/// Name bounds in surface units.
	pub rect: AvatarRect,
	/// Visible region in surface units.
	pub clip: AvatarRect,
}

/// Complete replacement of names drawn over one product connection.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(not(target_arch = "wasm32"), derive(uniffi::Record))]
pub struct PlacedContactLabels {
	/// Width of the product surface.
	pub surface_width: u32,
	/// Height of the product surface.
	pub surface_height: u32,
	/// Host-resolved names to draw. Empty clears the placement.
	pub labels: Vec<PlacedContactLabel>,
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
#[async_trait::async_trait]
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
	/// JS adapters apply the same unsupported default when the host omits UI.
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

	/// Edit the complete selection in host-owned UI. Cancellation is not an
	/// empty confirmed selection. Accounts and names stay host-side.
	async fn pick_contacts(
		&self,
		_product: &ProductContext,
		_selection: ContactSelection,
	) -> Result<HostContactsPick, GenericError> {
		Ok(HostContactsPick::Unsupported)
	}

	/// Draw names from the host's contact directory, with an account fallback
	/// when no username exists. Profile sharing must not affect labels.
	///
	/// Replace the connection's previous placement, and clear it on navigation
	/// or disconnect. On directory invalidation, clear stale names and refresh
	/// the live placement from current contacts. No per-contact result is returned.
	/// Returns whether this host supports label placement, never whether any
	/// individual contact resolved. JS adapters return false for omitted UI.
	async fn place_contact_labels(
		&self,
		_product: &ProductContext,
		_placed: PlacedContactLabels,
	) -> Result<bool, truapi::latest::HostContactsPlaceLabelsError> {
		Ok(false)
	}
}

/// How the host's scanner ended.
#[derive(Debug, Clone, Encode, Decode)]
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
	/// The requesting App or Widget is not the screen the user sees, or the
	/// host application is not in front, so the host opened nothing.
	NotVisible,
}

/// Host-owned viewfinder for QR codes and barcodes. Optional. The Swift and
/// Kotlin bridges and the JS `scanner` callbacks follow the same rules.
///
/// - Title the viewfinder with the product id. Show `request.hint` under it as the product's words.
/// - Pass every code the camera reads to a `ScanFilter` built from `request`, and never follow a
///   scanned link.
/// - Answer [`HostScan::NotVisible`] for an App or Widget that is not on screen, or for any product
///   while the host application is not in front. A Worker reaching the host already passed the
///   core's tap check.
/// - Close the viewfinder when the core drops the future, and close any still open before opening
///   another. A JS host is not told about a drop.
#[async_trait::async_trait]
pub trait ScannerPlatform: Send + Sync {
	/// Open the viewfinder on behalf of `product` and wait for the user.
	async fn scan_code(
		&self,
		product: &ProductContext,
		request: &HostScannerScanRequest,
	) -> Result<HostScan, GenericError>;
}

/// Combined platform interface. A host must provide every capability trait
/// listed here. Optional capabilities are declared separately in
/// [`OptionalPlatform`].
pub trait Platform:
	Navigation
	+ Notifications
	+ Permissions
	+ Features
	+ ExtrinsicVersionHost
	+ ProductStorage
	+ CoreStorage
	+ ChainProvider
	+ HopProvider
	+ NativeChatFilesHost
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
		+ ExtrinsicVersionHost
		+ ProductStorage
		+ CoreStorage
		+ ChainProvider
		+ HopProvider
		+ NativeChatFilesHost
		+ AuthPresenter
		+ UserConfirmation
		+ ThemeHost
		+ LocaleHost
		+ PreimageHost
		+ ProductOperations
{
}

/// Capability traits a host may serve but is not required to. A host that
/// omits one is not broken: the core normally answers the corresponding product
/// calls with `Unsupported`. [`CoinageWalletHost`] is the exception: absence
/// selects the built-in Rust wallet. Codegen reads this list to emit each
/// capability as an optional group on the host-callback surface.
pub trait OptionalPlatform:
	ChatPlatform
	+ ContactsPlatform
	+ PermissionStatusHost
	+ PocketPlatform
	+ ProfilePlatform
	+ IdentityBackendHost
	+ CoinageWalletHost
	+ GamePlatform
	+ ScannerPlatform
{
}

impl<T> OptionalPlatform for T where
	T: ChatPlatform
		+ ContactsPlatform
		+ PermissionStatusHost
		+ PocketPlatform
		+ ProfilePlatform
		+ IdentityBackendHost
		+ CoinageWalletHost
		+ GamePlatform
		+ ScannerPlatform
{
}
