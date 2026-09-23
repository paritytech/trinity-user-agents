#![allow(
    clippy::double_must_use,
    reason = "async-trait generates must_use futures for async trait methods"
)]

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

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "uniffi")]
uniffi::use_remote_type!(truapi::Bytes32);

use truapi::Bytes32;
use truapi::latest::{
    AllocatableResource, ChainIdentifier, ChatAction, ChatActions, ChatCustomMessage, ChatFile,
    ChatMedia, ChatMessageContent, ChatReaction, ChatRichText, DerivationIndex, GenericError,
    HostChatCreateRoomError, HostChatCreateRoomRequest, HostChatCreateRoomResponse,
    HostChatListSubscribeItem, HostChatPostMessageError, HostChatPostMessageRequest,
    HostChatPostMessageResponse, HostChatRegisterBotError, HostChatRegisterBotRequest,
    HostChatRegisterBotResponse, HostDevicePermissionRequest, HostDevicePermissionResponse,
    HostFeatureSupportedRequest, HostFeatureSupportedResponse, HostLocaleSubscribeItem,
    HostNativeChatAttachmentMetadata, HostNavigateToError, HostPlatform,
    HostPocketListSubscribeItem, HostPocketRemoveCardError, HostPocketRemoveCardRequest,
    HostPushNotificationRequest, HostPushNotificationResponse, HostSignPayloadRequest,
    HostSignPayloadWithLegacyAccountRequest, HostSignRawRequest,
    HostSignRawWithLegacyAccountRequest, HostThemeSubscribeItem, LegacyAccountTxPayload,
    NotificationId, ProductAccountId, ProductAccountTxPayload, ProductProofContext,
    RemotePermission, RemotePermissionRequest, RemotePermissionResponse, RingLocation,
};
use truapi::v01::HostAccountSignVrfRequest;
use url::{Host, Url};

/// Role-neutral runtime configuration supplied by the embedding host.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostRuntimeConfig {
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
    pub host: HostRuntimeConfig,
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
    pub host: HostRuntimeConfig,
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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

impl HostRuntimeConfig {
    /// Build a role-neutral host runtime config, validating fields whose
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
        Ok(Self {
            host_info,
            platform_info,
        })
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
            host: HostRuntimeConfig::new(host_info, platform_info)?,
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
            host: HostRuntimeConfig::new(host_info, platform_info)?,
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
        Ok(Self {
            product_id: normalize_product_identifier(&product_id)?,
            execution_kind,
        })
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
    normalized
        .rsplit_once('.')
        .is_some_and(|(_, tld)| DOTNS_TLDS.contains(&tld))
}

/// Bare product labels whose products hold every [`RemotePermission`] without a
/// user prompt.
///
/// These are first-party surfaces shipped alongside the host, so their remote
/// access belongs to the host's own trust boundary rather than to a per-product
/// decision. The list covers remote permissions only: device permissions,
/// identity disclosure and cross-product account access are always asked for.
/// Entries carry no TLD, so one entry covers the product on every network in
/// [`DOTNS_TLDS`].
pub const REMOTE_PERMISSION_TRUSTED_LABELS: &[&str] = &["peopl", "dim2", "stash"];

/// Whether `product_id` holds every [`RemotePermission`] without prompting.
///
/// Expects the [`normalize_product_identifier`] form. Matches the whole label
/// and nothing else: `peopl.dot` and `peopl.paseo` are trusted, while
/// `app.peopl.dot` and any `localhost` identifier are separate products and are
/// not. The label is only read out of an id that [`has_dotns_tld`] accepts, so a
/// widened product-id policy cannot promote an arbitrary single-label host.
pub fn has_trusted_remote_permissions(product_id: &str) -> bool {
    has_dotns_tld(product_id)
        && product_id
            .rsplit_once('.')
            .is_some_and(|(label, _tld)| REMOTE_PERMISSION_TRUSTED_LABELS.contains(&label))
}

/// Whether `product_id` in any accepted spelling holds every
/// [`RemotePermission`] without prompting.
///
/// [`has_trusted_remote_permissions`] reads the normalized form, which is what
/// the core always holds. A host holds whatever spelling it received, so this
/// normalizes first and answers `false` for an id that does not normalize at
/// all: an unrecognised spelling is never read as trusted.
///
/// Answers only whether the product is on the compiled-in list. A stored user
/// decision is not consulted here and always wins over it.
pub fn normalizes_to_trusted_remote_permissions(product_id: &str) -> bool {
    normalize_product_identifier(product_id)
        .is_ok_and(|normalized| has_trusted_remote_permissions(&normalized))
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
    if has_dotns_tld(&normalized)
        || normalized == "localhost"
        || normalized.starts_with("localhost:")
    {
        Ok(normalized)
    } else {
        Err(RuntimeConfigValidationError::InvalidProductId {
            product_id: product_id.to_string(),
        })
    }
}

/// Whether every dot-separated label of a normalized id is well formed.
///
/// Only the suffix after the last `.` was ever inspected, so the rest of the id
/// could be anything the transport carried. Three consequences, all reachable
/// because a cross-product call takes this string from the wire where it is
/// self-asserted:
///
/// - An empty label. `dim2..dot` and `.dot` both reduce to `""`, so distinct ids
///   collapse onto one grant key and one manifest cache entry.
/// - Control and format characters. A NUL or a right-to-left override survives
///   into [`AccountAccessReview`], which renders the id verbatim in the only
///   consent prompt this design has, and into every log line carrying it.
/// - Whitespace and path separators, which let one product's id render like
///   another's anywhere the comparison is not byte-exact.
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
        !label.is_empty()
            && !label.chars().any(|c| {
                c.is_control()
                    || c.is_whitespace()
                    || is_invisible_format(c)
                    || matches!(c, '/' | '\\')
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
const ALLOWED_ICON_DATA_TYPES: [&str; 5] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/avif",
];

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
        return Err(ChatFieldError::TooLong {
            field,
            limit: CHAT_FIELD_MAX_BYTES,
        });
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
        Content::Text { text } => Content::Text {
            text: validate_chat_body("text", &text)?,
        },
        Content::RichText(rich) => Content::RichText(ChatRichText {
            text: rich
                .text
                .map(|t| validate_chat_body("text", &t))
                .transpose()?,
            media: validate_chat_media("media", rich.media)?,
        }),
        Content::Actions(actions) => {
            if actions.actions.len() > CHAT_ACTIONS_MAX {
                return Err(ChatFieldError::TooMany {
                    field: "actions",
                    limit: CHAT_ACTIONS_MAX,
                });
            }
            Content::Actions(ChatActions {
                text: actions
                    .text
                    .map(|t| validate_chat_body("text", &t))
                    .transpose()?,
                actions: validate_chat_actions(actions.actions)?,
                layout: actions.layout,
            })
        }
        Content::File(file) => Content::File(ChatFile {
            url: validate_chat_url("url", &file.url)?,
            file_name: validate_chat_file_name("fileName", &file.file_name)?,
            mime_type: validate_chat_name("mimeType", &file.mime_type)?,
            size_bytes: file.size_bytes,
            text: file
                .text
                .map(|t| validate_chat_body("text", &t))
                .transpose()?,
        }),
        Content::Reaction(reaction) => Content::Reaction(validate_chat_reaction(reaction)?),
        Content::ReactionRemoved(reaction) => {
            Content::ReactionRemoved(validate_chat_reaction(reaction)?)
        }
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
        }
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
    if validated.contains(['/', '\\', ':'])
        || validated == ".."
        || validated == "."
        || validated.starts_with("..")
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
            Ok(ChatAction {
                action_id,
                title: validate_chat_name("title", &action.title)?,
            })
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
        return Err(ChatFieldError::TooMany {
            field,
            limit: CHAT_MEDIA_MAX,
        });
    }
    media
        .into_iter()
        .map(|item| {
            Ok(ChatMedia {
                url: validate_chat_url("url", &item.url)?,
            })
        })
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
        return Err(ChatFieldError::TooLong {
            field,
            limit: CHAT_BODY_MAX_BYTES,
        });
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
        return Err(ChatFieldError::TooLong {
            field,
            limit: CHAT_FIELD_MAX_BYTES,
        });
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
                return Err(ChatFieldError::TooLong {
                    field,
                    limit: CHAT_ICON_MAX_BYTES,
                });
            }
            Ok(trimmed.to_string())
        }
        _ => Err(ChatFieldError::RejectedScheme { field }),
    }
}

/// Validate a product-supplied chat icon: absent, an `https` URL, or an inline
/// image in [`ALLOWED_ICON_DATA_TYPES`].
///
/// An allowlist rather than a denylist, because a URL parser reaches a scheme
/// through whitespace, tabs and NUL that a prefix comparison does not.
pub fn validate_chat_icon(field: &'static str, icon: &str) -> Result<String, ChatFieldError> {
    let trimmed = icon.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > CHAT_ICON_MAX_BYTES {
        return Err(ChatFieldError::TooLong {
            field,
            limit: CHAT_ICON_MAX_BYTES,
        });
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
    if scheme.is_empty()
        || !scheme
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
    character.is_control()
        || matches!(
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
    /// Carries lengths rather than the id, which may be enormous. Appended
    /// because the native mirror maps variants to FFI discriminants by order.
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
        Ok(Self {
            product_id: normalize_product_identifier(product_id)?,
            key: key.into(),
        })
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
/// The core namespaces product keys before calling this trait. Host
/// implementations may treat `key` as opaque or decode it with
/// [`ProductStorageKey`] when their physical storage is separated by product.
/// Storage errors are pinned to `v01` rather than taken from `truapi::latest`.
/// The read error gained a cross-product refusal in v0.2 that the core decides
/// before it ever calls a host, so a host has no way to produce it and should
/// not have to match on it.
#[async_trait]
pub trait ProductStorage: Send + Sync {
    /// Read a value by key.
    ///
    /// Always the calling product's own storage. A read addressed at another
    /// product is adjudicated in the core against that product's manifest and
    /// refused there, so a host is never asked to enforce a grant and has no
    /// variant for one.
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
    async fn cancel_notification(&self, id: NotificationId) -> Result<(), GenericError> {
        let _ = id;
        Ok(())
    }
}

/// Permission prompts. Device permissions (camera, mic, NFC, ...) are separate
/// from remote permissions (domain access, chain submit, ...), so the platform
/// surface mirrors that split.
#[async_trait]
pub trait Permissions: Send + Sync {
    /// Prompt the user for a device-level permission.
    async fn device_permission(
        &self,
        request: HostDevicePermissionRequest,
    ) -> Result<HostDevicePermissionResponse, GenericError>;

    /// Prompt the user for a remote (product-scoped) permission bundle.
    async fn remote_permission(
        &self,
        request: RemotePermissionRequest,
    ) -> Result<RemotePermissionResponse, GenericError>;
}

/// Permission request whose authorization status can be inspected or updated
/// by host administration UI.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
    /// Product-scoped permission to bind and use wallet-held Chat identity authority.
    #[codec(index = 4)]
    ChatAuthority,
    /// Product-scoped permission to ensure Statement Store quota, not increase it.
    #[codec(index = 5)]
    StatementStoreAllowance {
        /// `None` selects the legacy allowance account; `Some` selects a product account.
        derivation_index: Option<DerivationIndex>,
    },
}

/// Authorization status for a permission request.
///
/// `NotDetermined` means the core has no persisted answer and will prompt the
/// host the next time the product requests this permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HostChainEntry {
    /// Protocol role this entry answers for.
    pub identifier: ChainIdentifier,
    /// Genesis hash identifying the chain in all chain-scoped calls.
    pub genesis_hash: Bytes32,
}

/// The chain set a host serves: its environment plus one entry per chain role.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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

/// JSON-RPC provider factory for chain access.
///
/// The platform provides a way to get a JSON-RPC connection for a given chain.
/// The server runtime manages the chainHead v1 state machine on top of this.
/// Host-spec N.6 requires products to access chains through host-mediated
/// providers:
/// <https://github.com/paritytech/host-spec/blob/adb3989208ae1c2107dbf0159611353e6989422c/spec/N-shared-infrastructure.md?plain=1#L91-L102>
#[async_trait]
pub trait ChainProvider: Send + Sync {
    /// Open a JSON-RPC connection for the chain identified by `genesis_hash`.
    /// Drop the returned connection to disconnect.
    async fn connect(
        &self,
        genesis_hash: [u8; 32],
    ) -> Result<Box<dyn JsonRpcConnection>, GenericError>;
}

/// Wallet and asset binding checked by the native service before every operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NativeCoinageMemo {
    /// Validated 64-byte native sr25519 secret keys, confined to the trusted Host.
    pub secret_keys: Vec<Vec<u8>>,
    /// Exact total in canonical decimal raw chain units.
    pub total_value_raw: String,
}

/// Durable native-wallet operations, not an alternative inventory ledger.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
        if let Self::Prepared {
            memo: Some(memo), ..
        } = self
        {
            zeroize::Zeroize::zeroize(memo);
        }
    }
}

/// Optional native wallet service boundary, never exposed to products.
///
/// Injecting this service at runtime construction assigns native custody for
/// the runtime's lifetime. Absence selects the built-in Rust wallet; native
/// unavailability or failure never permits Rust fallback.
#[async_trait]
pub trait CoinageWalletHost: Send + Sync {
    /// Invoke the selected native owner. An error never permits Rust fallback.
    async fn native_coinage(
        &self,
        request: NativeCoinageRequest,
    ) -> Result<NativeCoinageResponse, GenericError>;
}

/// Trusted native Chat selection context; never passed to a product.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct NativeChatPickedFile {
    /// Opaque private handle surviving restart until explicitly released.
    pub source_id: String,
    /// Actual source size and native media metadata.
    pub metadata: HostNativeChatAttachmentMetadata,
}

/// Trusted context for exporting a verified native Chat attachment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[async_trait]
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
#[async_trait]
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
        Err(GenericError {
            reason: "HOP provider unavailable".to_string(),
        })
    }
}

/// Check an endpoint without normalizing it into a different allowlist entry.
pub fn ensure_allowed_hop_endpoint(endpoint: &str, allowed: &[String]) -> Result<(), GenericError> {
    let valid = endpoint.starts_with("wss://")
        && !endpoint
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
        && !endpoint.contains(['#', '\\'])
        && !endpoint[6..]
            .split(['/', '?'])
            .next()
            .unwrap_or_default()
            .contains('@')
        && url::Url::parse(endpoint).is_ok_and(|url| {
            url.scheme() == "wss"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
                && url.fragment().is_none()
        })
        && allowed.iter().any(|entry| entry == endpoint);
    if !valid {
        return Err(GenericError {
            reason: "HOP endpoint is not in the trusted Bulletin WSS allowlist".to_string(),
        });
    }
    Ok(())
}

/// A live JSON-RPC connection to a host-selected service.
pub trait JsonRpcConnection: Send + Sync {
    /// Send a JSON-RPC request string.
    fn send(&self, request: String);

    /// Stream of JSON-RPC response strings.
    fn responses(&self) -> BoxStream<'static, String>;

    /// Close the connection lease.
    ///
    /// Hosts may keep a shared underlying transport alive, but this handle
    /// must stop receiving responses and release any per-caller resources.
    fn close(&self);
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
    /// Encrypted main-purse inventory, operation journal, and claim recovery state.
    ///
    /// This slot is wallet-owned, not product-owned, and must survive clearing a
    /// product's data. Writes replace the entire value atomically.
    #[codec(index = 13)]
    MainPurseCoinage {
        /// Root public key identifying the wallet.
        root_public_key: [u8; 32],
        /// Chain whose Coinage inventory is recorded.
        genesis_hash: [u8; 32],
    },
    /// Encrypted Host-owned native Chat device, peer roster, and migration state.
    #[codec(index = 14)]
    NativeChatDevice {
        /// Wallet owning the Chat identity.
        root_public_key: [u8; 32],
        /// Host-selected Chat network.
        genesis_hash: [u8; 32],
        /// Authenticated product using the device.
        product_id: String,
    },
    /// One encrypted, bounded native Chat attachment cache chunk.
    /// Values use the Chat state's wallet-bound authenticated encryption.
    #[codec(index = 15)]
    NativeChatFileChunk {
        /// Wallet owning the attachment.
        root_public_key: [u8; 32],
        /// Host-selected Chat network.
        genesis_hash: [u8; 32],
        /// Authenticated product using the device.
        product_id: String,
        /// Public opaque attachment identifier, not a source handle or ticket.
        attachment_id: [u8; 32],
        /// Exact chunk index within the authenticated file root.
        chunk_index: u32,
    },
    /// Previously initialized Chat products to restore after this wallet unlocks.
    ///
    /// This is a host-private wallet-owned index, not a permission grant. The
    /// core rechecks each product's current grants before restoring reception.
    #[codec(index = 16)]
    NativeChatProducts {
        /// Wallet owning the installed Chat devices.
        root_public_key: [u8; 32],
        /// Host-selected Chat network.
        genesis_hash: [u8; 32],
    },
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
        }
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
        CoreStorageKey::MainPurseCoinage { .. } => ("MainPurseCoinage", None),
        CoreStorageKey::NativeChatDevice { .. } => ("NativeChatDevice", None),
        CoreStorageKey::NativeChatProducts { .. } => ("NativeChatProducts", None),
        CoreStorageKey::NativeChatFileChunk { product_id, .. } => {
            ("NativeChatFileChunk", Some(product_id))
        }
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
                permission: RemotePermission::Remote {
                    domains: vec![domain.to_string()],
                },
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

    /// Persisted authorization key for wallet-held Chat identity authority.
    pub fn chat_authority_authorization(product_id: &str) -> Self {
        Self::PermissionAuthorization {
            product_id: product_id.to_string(),
            request: PermissionAuthorizationRequest::ChatAuthority,
        }
    }

    /// Persisted authorization for one statement allowance account selector.
    pub fn statement_store_allowance_authorization(
        product_id: &str,
        derivation_index: Option<DerivationIndex>,
    ) -> Self {
        Self::PermissionAuthorization {
            product_id: product_id.to_string(),
            request: PermissionAuthorizationRequest::StatementStoreAllowance { derivation_index },
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

/// Stored domain patterns that would authorize outbound access to `host`,
/// ordered most specific first.
///
/// Implements the RFC 0002 matching rules: an exact host match, a single-level
/// wildcard over the host's immediate parent, and the universal wildcard. Two
/// consequences worth holding onto, because both are load-bearing:
///
/// - A wildcard spans exactly one label. `*.example.com` authorizes
///   `api.example.com` but not `deep.api.example.com`, whose only wildcard
///   candidate is `*.api.example.com`.
/// - A bare parent domain is never a candidate. Granting `example.com` does not
///   extend to `api.example.com`; that needs the explicit host or the wildcard.
///
/// Every pattern a product can be granted is consulted here, including a
/// TLD-level one such as `*.com` or `*.dot`. Narrowing the candidate list
/// instead would store such a grant and then never read it, so the product
/// would keep prompting for every host under a pattern the user already
/// approved. Breadth is the prompt's problem: RFC 0002 already puts the duty of
/// spelling out how wide `*` is on the host UI, and a TLD wildcard belongs in
/// the same sentence.
///
/// Ordering is the precedence rule for the caller: the most specific stored
/// decision wins, so an explicit grant for one host survives a denial of its
/// parent wildcard, and vice versa.
pub fn remote_domain_candidates(host: &str) -> Vec<String> {
    let normalized = normalize_remote_domain(host);
    let mut candidates = vec![normalized.clone()];
    if let Some((_label, parent)) = normalized.split_once('.') {
        candidates.push(format!("*.{parent}"));
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
            let mut canonical: Vec<String> = domains
                .iter()
                .map(|domain| normalize_remote_domain(domain))
                .collect();
            canonical.sort();
            canonical.dedup();
            RemotePermission::Remote { domains: canonical }
        }
        other => other.clone(),
    };
    RemotePermissionRequest { permission }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hop_dialing_requires_exact_secure_endpoint_membership() {
        let allowed = vec!["wss://hop.example/rpc".to_string()];
        assert!(ensure_allowed_hop_endpoint(&allowed[0], &allowed).is_ok());
        for endpoint in [
            "wss://hop.example/rpc/",
            "wss://HOP.example/rpc",
            "wss://hop.example:443/rpc",
            "wss://hop.example/rpc?other=1",
            "wss://hop.example.evil/rpc",
        ] {
            assert!(ensure_allowed_hop_endpoint(endpoint, &allowed).is_err());
        }
        // Even malformed trusted configuration must not weaken the dial policy.
        for endpoint in [
            "ws://hop.example/rpc",
            "https://hop.example/rpc",
            "wss://user:secret@hop.example/rpc",
            "wss://@hop.example/rpc",
            "wss://hop.example\\rpc",
            "wss://hop.example/rpc#fragment",
            "wss://hop.example/\nrpc",
            " wss://hop.example/rpc",
        ] {
            assert!(ensure_allowed_hop_endpoint(endpoint, &[endpoint.to_string()]).is_err());
        }
    }

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
            Err(RuntimeConfigValidationError::EmptyField {
                field: "network_suffix"
            })
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
                .map(|index| ChatAction {
                    action_id: format!("a{index}"),
                    title: "go".to_string(),
                })
                .collect(),
            layout: truapi::latest::ChatActionLayout::Column,
        });
        assert_eq!(
            validate_chat_message_content(too_many),
            Err(ChatFieldError::TooMany {
                field: "actions",
                limit: CHAT_ACTIONS_MAX,
            })
        );

        let too_long = ChatMessageContent::Text {
            text: "x".repeat(CHAT_BODY_MAX_BYTES + 1),
        };
        assert_eq!(
            validate_chat_message_content(too_long),
            Err(ChatFieldError::TooLong {
                field: "text",
                limit: CHAT_BODY_MAX_BYTES,
            })
        );

        let too_much_media = ChatMessageContent::RichText(ChatRichText {
            text: None,
            media: (0..CHAT_MEDIA_MAX + 1)
                .map(|_| ChatMedia {
                    url: "https://example.invalid/m".to_string(),
                })
                .collect(),
        });
        assert_eq!(
            validate_chat_message_content(too_much_media),
            Err(ChatFieldError::TooMany {
                field: "media",
                limit: CHAT_MEDIA_MAX,
            })
        );
    }

    #[test]
    fn a_message_body_carries_the_text_a_person_typed() {
        // A chat message is written text: line breaks and tabs are content,
        // and the bytes must survive so a product can echo-compare them.
        let markdown = "# Report\n\n| a | b |\n| - | - |\n\tindented\n";
        assert_eq!(
            validate_chat_message_content(ChatMessageContent::Text {
                text: markdown.to_string(),
            }),
            Ok(ChatMessageContent::Text {
                text: markdown.to_string(),
            })
        );

        // Neither trimmed nor NFC-normalized.
        let unnormalized = "  cafe\u{301}  ";
        assert_eq!(
            validate_chat_message_content(ChatMessageContent::Text {
                text: unnormalized.to_string(),
            }),
            Ok(ChatMessageContent::Text {
                text: unnormalized.to_string(),
            })
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
            media: vec![ChatMedia {
                url: "javascript:alert(1)".to_string(),
            }],
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
            Err(ChatFieldError::Empty {
                field: "messageType"
            })
        );

        let payload = ChatMessageContent::Custom(ChatCustomMessage {
            message_type: "vote".to_string(),
            payload: vec![0; CHAT_CUSTOM_PAYLOAD_MAX_BYTES + 1],
        });
        assert_eq!(
            validate_chat_message_content(payload),
            Err(ChatFieldError::TooLong {
                field: "payload",
                limit: CHAT_CUSTOM_PAYLOAD_MAX_BYTES,
            })
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
            Err(ChatFieldError::TooLong {
                field: "url",
                limit: CHAT_URL_MAX_BYTES,
            })
        );
    }

    #[test]
    fn action_ids_that_normalize_alike_are_rejected() {
        // Normalization maps these onto one key. Accepting both would give the
        // user two buttons whose trigger the product cannot tell apart.
        let colliding = ChatMessageContent::Actions(ChatActions {
            text: None,
            actions: vec![
                ChatAction {
                    action_id: "approve".to_string(),
                    title: "Approve".to_string(),
                },
                ChatAction {
                    action_id: " approve ".to_string(),
                    title: "Reject".to_string(),
                },
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
                ChatAction {
                    action_id: "approve".to_string(),
                    title: "Approve".to_string(),
                },
                ChatAction {
                    action_id: "approve".to_string(),
                    title: "Reject".to_string(),
                },
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
            Url::parse(&icon)
                .expect("a parsable https url")
                .to_string()
                .len()
                > CHAT_ICON_MAX_BYTES,
            "resolves past it"
        );

        assert_eq!(
            validate_chat_icon("icon", &icon),
            Err(ChatFieldError::TooLong {
                field: "icon",
                limit: CHAT_ICON_MAX_BYTES,
            })
        );

        let message_url = format!(
            "https://files.invalid/{}",
            "\u{00e9}".repeat(CHAT_URL_MAX_BYTES / 4)
        );
        assert!(message_url.len() <= CHAT_URL_MAX_BYTES);
        assert_eq!(
            validate_chat_url("url", &message_url),
            Err(ChatFieldError::TooLong {
                field: "url",
                limit: CHAT_URL_MAX_BYTES,
            })
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
        assert_eq!(
            context.encode(),
            [28, b'a', b'p', b'p', b'.', b'd', b'o', b't', 2]
        );
        assert_eq!(
            ProductContext::decode(&mut context.encode().as_slice()),
            Ok(context)
        );
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
    fn trusted_remote_permissions_normalize_the_spellings_a_host_holds() {
        // Both the UniFFI and wasm exports answer through this, and a host
        // holds an id in whatever spelling it received rather than the
        // normalized form the core passes internally.
        for product_id in [
            "peopl.dot",
            "PEOPL.DOT",
            "  peopl.dot  ",
            "dim2.paseo",
            "stash.dot",
        ] {
            assert!(
                normalizes_to_trusted_remote_permissions(product_id),
                "{product_id} is a first-party product in any accepted spelling"
            );
        }
        for product_id in [
            "app.peopl.dot",
            "peopl",
            "notpeopl.dot",
            "localhost:3000",
            "",
            "   ",
        ] {
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
            assert!(
                normalize_product_identifier(id).is_ok(),
                "{id:?} is a legitimate product id"
            );
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
        assert!(
            is_product_identifier(&at_cap),
            "an id exactly at the cap is still valid"
        );
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
            (
                CoreStorageKey::PairingDeviceIdentity,
                "PairingDeviceIdentity",
                None,
            ),
            (
                CoreStorageKey::AllowanceKeys {
                    session_id: "session".to_string(),
                },
                "AllowanceKeys",
                None,
            ),
            (
                CoreStorageKey::LastProcessedPairingStatement,
                "LastProcessedPairingStatement",
                None,
            ),
            (
                CoreStorageKey::AutoSigningKey {
                    product_id: "product.dot".to_string(),
                },
                "AutoSigningKey",
                Some("product.dot"),
            ),
            (CoreStorageKey::AutoSigningKeys, "AutoSigningKeys", None),
            (
                CoreStorageKey::RingVrfRegistry {
                    root_public_key: [0x42; 32],
                },
                "RingVrfRegistry",
                None,
            ),
            (
                CoreStorageKey::StatementRenewalTargets,
                "StatementRenewalTargets",
                None,
            ),
            (
                CoreStorageKey::DeviceEncryptionKey,
                "DeviceEncryptionKey",
                None,
            ),
            (
                CoreStorageKey::SsoResponderRequestLedger {
                    root_public_key: [0x11; 32],
                    peer_statement_account_id: [0x22; 32],
                    peer_encryption_public_key: [0x33; 32],
                },
                "SsoResponderRequestLedger",
                None,
            ),
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
            &RemotePermissionRequest {
                permission: RemotePermission::ChainSubmit,
            },
        );
        let identity = CoreStorageKey::identity_disclosure_authorization("product.dot");
        let other_product_identity = CoreStorageKey::identity_disclosure_authorization("other.dot");
        let account_access =
            CoreStorageKey::account_access_authorization("product.dot", "target.dot");
        let other_target = CoreStorageKey::account_access_authorization("product.dot", "other.dot");
        let chat_authority = CoreStorageKey::chat_authority_authorization("product.dot");
        let other_product_chat = CoreStorageKey::chat_authority_authorization("other.dot");

        assert_ne!(camera, other_product);
        assert_ne!(camera, remote);
        assert_ne!(camera, identity);
        assert_ne!(remote, identity);
        assert_ne!(identity, other_product_identity);
        assert_ne!(account_access, other_target);
        assert_ne!(account_access, camera);
        assert_ne!(chat_authority, identity);
        assert_ne!(chat_authority, account_access);
        assert_ne!(chat_authority, other_product_chat);
    }

    #[test]
    fn remote_domain_candidates_follow_rfc_0002_wildcard_rules() {
        assert_eq!(
            remote_domain_candidates("api.example.com"),
            ["api.example.com", "*.example.com", "*"]
        );
        // A wildcard spans one label, so the two-level host's only wildcard is
        // over its immediate parent. `*.example.com` must NOT appear here.
        assert_eq!(
            remote_domain_candidates("deep.api.example.com"),
            ["deep.api.example.com", "*.api.example.com", "*"]
        );
        // A TLD-level wildcard is a pattern a product can be granted, so it is
        // consulted like any other. Leaving it out would store the grant and
        // then keep prompting for every host under it.
        assert_eq!(
            remote_domain_candidates("example.com"),
            ["example.com", "*.com", "*"]
        );
        assert_eq!(
            remote_domain_candidates("wallet.dot"),
            ["wallet.dot", "*.dot", "*"]
        );
        // A single-label host has no parent to wildcard over.
        assert_eq!(remote_domain_candidates("localhost"), ["localhost", "*"]);
        // A stored pattern resolves to itself, not to a duplicated entry.
        assert_eq!(
            remote_domain_candidates("*.example.com"),
            ["*.example.com", "*"]
        );
        assert_eq!(remote_domain_candidates("*"), ["*"]);
        assert_eq!(
            remote_domain_candidates("API.Example.COM"),
            ["api.example.com", "*.example.com", "*"]
        );
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
        assert_eq!(
            normalize_remote_domain("Bücher.example"),
            "xn--bcher-kva.example"
        );
        assert_eq!(
            normalize_remote_domain("xn--bcher-kva.example"),
            "xn--bcher-kva.example"
        );
        // The wildcard marker is not part of the host and survives untouched.
        assert_eq!(normalize_remote_domain("*.Example.COM"), "*.example.com");
        assert_eq!(
            normalize_remote_domain("*.Bücher.example"),
            "*.xn--bcher-kva.example"
        );
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

        let webrtc = RemotePermissionRequest {
            permission: RemotePermission::WebRtc,
        };
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
/// [`CoreStorageKeyDescription::product_id`] is `Some` for product-indexed
/// slots, including the encrypted attachment chunk cache. Wallet-owned state
/// remains outside that sweep. Keying host storage by this metadata makes
/// product removal a prefix delete rather than a scan.
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
    /// Short username from the dotNS identity record on Asset Hub.
    pub lite_username: Option<String>,
    /// Fully qualified username from the dotNS identity record on Asset Hub.
    pub full_username: Option<String>,
}

/// Auth/session lifecycle state the core projects for host UI. The core owns
/// every transition and emits states in order; hosts render the current state
/// and never derive auth UI from any other signal.
#[derive(Debug, Clone, Default, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SignPayloadReview {
    /// Product-account signing request.
    Product(HostSignPayloadRequest),
    /// Legacy-account signing request.
    LegacyAccount(HostSignPayloadWithLegacyAccountRequest),
}

/// Review shown before a sign-raw request is sent to the paired wallet.
/// Hosts must display the payload according to `watermarked` and warn that
/// unwatermarked signatures can authorize transactions.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum SignRawReview {
    /// Product-account raw signing request.
    Product {
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct StatementStoreProductSignReview {
    /// Product account that will sign the statement payload.
    pub account: ProductAccountId,
    /// Exact unsigned statement payload to be signed.
    pub payload: Vec<u8>,
}

/// Review shown before a transaction-creation request is sent to the paired wallet.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum CreateTransactionReview {
    /// Product-account transaction request.
    Product(ProductAccountTxPayload),
    /// Legacy-account transaction request.
    LegacyAccount(LegacyAccountTxPayload),
}

/// Review shown before a product derives a contextual alias (RFC 0004).
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ResourceAllocationReview {
    /// Product the allocation is requested for.
    pub calling_product_id: String,
    /// Resources to allocate.
    pub resources: Vec<AllocatableResource>,
}

/// Review shown before a product asks to access another product account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct AccountAccessReview {
    /// Product currently handling the request.
    pub requesting_product_id: String,
    /// Product whose account is being requested.
    pub target_product_id: String,
}

/// Review shown before a product learns the user's primary identity.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct IdentityDisclosureReview {
    /// Product currently handling the request.
    pub product_id: String,
}

/// Review shown before a product binds or uses wallet-held Chat identity authority.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ChatAuthorityReview {
    /// Product requesting the Chat identity operation.
    pub product_id: String,
}

/// Exact Host-resolved payment reviewed before debiting the user's main purse.
///
/// This review never grants a reusable spending permission. Chat authority and
/// automatic product signing do not authorize it.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
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
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct ProductSubtreeReview {
    /// Product resolving its own account.
    pub product_id: String,
}

/// Review shown before a preimage is submitted.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct PreimageSubmitReview {
    /// Size of the preimage in bytes.
    pub size: u64,
}

/// Review shown before a user-confirmed core action continues.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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
    /// Submit a preimage to the host-selected backend.
    PreimageSubmit(PreimageSubmitReview),
    /// Allow a product to access another product account.
    AccountAccess(AccountAccessReview),
    /// Sign an RFC-0023 VRF transcript with a product account.
    SignVrf(SignVrfReview),
    /// Resolve a product's own account subtree over SSO.
    ProductSubtree(ProductSubtreeReview),
    /// Allow a product to bind and use wallet-held Chat identity authority.
    ChatAuthority(ChatAuthorityReview),
    /// Confirm this exact main-purse payment; never eligible for auto-approval.
    MainPurseChatPayment(MainPurseChatPaymentReview),
}

/// Local user confirmation UI for sensitive core-owned operations.
#[async_trait]
pub trait UserConfirmation: Send + Sync {
    /// Confirm a reviewed action before the core continues.
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

/// Optional host-authenticated username candidate source. The host owns backend
/// configuration and credentials. Candidates are never ownership assertions:
/// the core checks finalized dotNS ownership and the canonical People Chat key.
#[async_trait]
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
/// names and icons on `create_chat_room`, `register_chat_bot` and
/// `post_chat_message` are NFC-normalized and rejected for control and bidi
/// characters. Message bodies are bounded and screened but pass through
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

    /// Emit the current product-scoped room list and later replacements.
    fn subscribe_chat_rooms(
        &self,
        product: &ProductContext,
    ) -> BoxStream<'static, Result<HostChatListSubscribeItem, GenericError>>;
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

/// What the operating system currently says about a device capability.
///
/// Distinct from [`PermissionAuthorizationStatus`], which is the product-scoped
/// decision the user made through TrUAPI. The two answer different questions
/// and are combined rather than substituted: a capability is usable only when
/// the product holds a grant *and* the OS still allows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
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

/// Combined platform interface. A host must provide every capability trait
/// listed here. Optional capabilities are declared separately in
/// [`OptionalPlatform`].
pub trait Platform:
    Navigation
    + Notifications
    + Permissions
    + Features
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
        + HopProvider
        + NativeChatFilesHost
        + AuthPresenter
        + UserConfirmation
        + ThemeHost
        + LocaleHost
        + PreimageHost
{
}

/// Capability traits a host may serve but is not required to. A host that
/// omits one is not broken: the core normally answers the corresponding product
/// calls with `Unsupported`. [`CoinageWalletHost`] is the exception: absence
/// selects the built-in Rust wallet. Codegen reads this list to emit each
/// capability as an optional group on the host-callback surface.
pub trait OptionalPlatform:
    ChatPlatform + PermissionStatusHost + PocketPlatform + IdentityBackendHost + CoinageWalletHost
{
}

impl<T> OptionalPlatform for T where
    T: ChatPlatform
        + PermissionStatusHost
        + PocketPlatform
        + IdentityBackendHost
        + CoinageWalletHost
{
}
