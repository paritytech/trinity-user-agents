//! UniFFI-facing native bridge. Exposes [`NativeTrUApiHostRuntime`],
//! [`NativeProductExecution`], and the [`HostCallbacks`] callback interface
//! that iOS and Android call into.
//!
//! The native side builds a `CallbackPlatform` that adapts every
//! [`crate::platform::Platform`] trait to a corresponding callback. The
//! resulting platform is fed into [`SigningHostRuntime`] so the rest of the
//! dispatcher pipeline behaves identically to the WS-bridge and wasm flavors.
//! A native host therefore owns the signer, so it never pairs itself with a
//! wallet and the pairing-host entry points are inert. It does answer other
//! devices pairing with it: the signing host's responder side is exposed here,
//! from the handshake answer through serving and ending the session.
//!
//! Contacts install once on the runtime rather than per execution, because the
//! book belongs to the host and not to any product: see
//! [`NativeTrUApiHostRuntime::set_contacts_callbacks`]. A host that installs
//! none leaves `contacts.pick` answering `Unsupported`.

mod callbacks;
mod config;
mod errors;
mod events;
mod executor;
mod platform;
mod renderer;
mod runtime;
mod ws_bridge;

pub use crate::host_internal::sso_messages::SsoRequestOutcome;
pub use crate::host_logic::dotns::{NavigateDecision, PocketDeeplinkAction};
pub use callbacks::{
    HostCallbacks, NativeChatCallbacks, NativeContactsCallbacks, NativeGameCallbacks,
    NativePocketCallbacks, NativePocketRemoval,
};
pub use config::{HostRuntimeConfig, NativeRuntimeConfigError, ProductExecutionConfig};
pub use errors::{HostRejection, NativeChatFieldError, NativeCoreDatabaseError, NativeRendererError};
pub use renderer::{NativeRendererObserver, NativeRendererSubscription};
pub use runtime::{
    NativeAnnouncedPairing, NativePairingError, NativeProductExecution, NativeTrUApiHostRuntime,
};
pub use ws_bridge::{WsBridgeEndpoint, WsBridgeStartError};

use parity_scale_codec::{DecodeLimit, Encode};
use serde::Deserialize;
use truapi::latest;

use crate::PairingProposal;
use crate::host_logic::dotns;
#[cfg(doc)]
use crate::SigningHostRuntime;

/// Classify a navigation input exactly like the core's internal navigate host
/// call: dotNS first, then `localhost`, then normalized external, with
/// everything else rejected. Pure and stateless; hosts call it on every
/// webview-internal navigation.
#[uniffi::export]
pub fn parse_navigate(input: String) -> NavigateDecision {
    dotns::parse_navigate(&input)
}

/// The bridge script a host injects into a product's web view, for the `port`
/// and `token` a `WsBridgeEndpoint` carries.
///
/// Inject it at document start, before the product's own scripts and before the
/// lockdown container, which reads the endpoint this publishes.
#[uniffi::export]
pub fn localhost_bridge_bootstrap_script(port: u16, token: String) -> String {
    crate::bootstrap::script(&format!("ws://127.0.0.1:{port}/?t={token}"))
}

/// Read what a pairing deeplink offers: the peer it advertises, and how that
/// peer describes itself.
///
/// A host needs the peer's `statement_account_id` before it answers: the
/// peer's device statement account has to be a tracked renewal target by the
/// time the session opens, or the peer has no allowance to author its own
/// session statements under. It is also what a failed pairing untracks again,
/// unless the device was already paired. Neither the notice, the answer, nor
/// [`HostCallbacks::device_paired`] yields it in time for that, so the host
/// reads it here first.
///
/// The core prompts for nothing here, so the pairing prompt is the host's, and
/// [`crate::PairingProposalMetadata`] is what it names the peer by. It arrives
/// sanitized for rendering but unverified: nothing signs it, so it says what
/// the peer calls itself and not who it is.
///
/// Pure and stateless, and the same decoder the responder itself runs, so a
/// deeplink this rejects is one no pairing call would have accepted either.
#[uniffi::export]
pub fn parse_pairing_deeplink(deeplink: String) -> Result<PairingProposal, NativePairingError> {
    PairingProposal::from_deeplink(&deeplink)
        .map_err(|reason| NativePairingError::UndecodableDeeplink { reason })
}

/// Refuse a deeplink the core's decoder would refuse anyway, so the caller
/// hears [`NativePairingError::UndecodableDeeplink`] rather than the
/// [`NativePairingError::Rejected`] every later failure shares.
fn reject_undecodable_deeplink(deeplink: &str) -> Result<(), NativePairingError> {
    PairingProposal::from_deeplink(deeplink)
        .map(|_| ())
        .map_err(|reason| NativePairingError::UndecodableDeeplink { reason })
}

/// Largest nesting a face may carry, matching what the core accepts on the
/// renderer subscription. A host with a shallower bound would draw a live face
/// it could not read back.
const MAX_FACE_DEPTH: u32 = 64;

/// Bracket nesting a face of [`MAX_FACE_DEPTH`] renderer levels can reach. One
/// level spends a few brackets on its object, its value and its children, so
/// this is generous on purpose: it guards the stack, while `MAX_FACE_DEPTH` is
/// what actually decides whether a face is drawable.
const MAX_FACE_JSON_NESTING: u32 = MAX_FACE_DEPTH * 8;

/// Read a product-declared card face: a `RendererNode` tree in the JSON shape
/// the generated TypeScript client describes.
///
/// Hosts call this rather than parsing the shape themselves. It is a protocol
/// format, so a host that reads it its own way disagrees with the other hosts
/// about which faces are drawable, and about how deep one may nest.
#[uniffi::export]
pub fn parse_renderer_node_json(json: String) -> Result<latest::RendererNode, NativeRendererError> {
    // Bounded before it is parsed, not after: serde_json recurses as it reads,
    // so a tree built to exhaust the stack would do so before any check on the
    // value it produced. Counting brackets needs no recursion at all.
    if json_nesting_exceeds(&json, MAX_FACE_JSON_NESTING) {
        return Err(NativeRendererError::TooDeep {
            limit: MAX_FACE_DEPTH,
        });
    }

    // With the text bounded above, the reader's own limit would only impose a
    // second, stricter bound in JSON levels rather than in renderer levels.
    let mut reader = serde_json::Deserializer::from_str(&json);
    reader.disable_recursion_limit();
    let node = latest::RendererNode::deserialize(&mut reader).map_err(|error| {
        NativeRendererError::Malformed {
            reason: error.to_string(),
        }
    })?;

    match node_depth(&node, MAX_FACE_DEPTH) {
        Some(_) => Ok(node),
        None => Err(NativeRendererError::TooDeep {
            limit: MAX_FACE_DEPTH,
        }),
    }
}

/// Whether `json` nests deeper than `limit` braces or brackets, ignoring the
/// ones inside strings. Iterative, so measuring a hostile tree costs no stack.
fn json_nesting_exceeds(json: &str, limit: u32) -> bool {
    let (mut depth, mut in_string, mut escaped) = (0u32, false, false);

    for byte in json.bytes() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match byte {
            b'"' => in_string = true,
            b'{' | b'[' => {
                depth += 1;
                if depth > limit {
                    return true;
                }
            }
            b'}' | b']' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }

    false
}

/// Depth of `node`, or `None` once it passes `remaining`. Bounded rather than
/// measured, so a tree built to exhaust the stack is refused before it does.
fn node_depth(node: &latest::RendererNode, remaining: u32) -> Option<u32> {
    if remaining == 0 {
        return None;
    }

    let children: &[latest::RendererNode] = match node {
        latest::RendererNode::Box { children, .. }
        | latest::RendererNode::Column { children, .. }
        | latest::RendererNode::Row { children, .. }
        | latest::RendererNode::Text { children, .. }
        | latest::RendererNode::Button { children, .. }
        | latest::RendererNode::Effect { children, .. } => children,
        _ => &[],
    };

    let deepest = children
        .iter()
        .map(|child| node_depth(child, remaining - 1))
        .try_fold(0, |deepest: u32, depth| depth.map(|d| deepest.max(d)))?;

    Some(deepest + 1)
}

/// The bytes to keep a face under, so it can be drawn again at a cold start.
///
/// SCALE, the encoding the tree already travels in, rather than a shape a host
/// invents for its own store: a host that writes its own cannot read back what
/// the protocol later adds, and two hosts disagree about what they kept.
#[uniffi::export]
pub fn encode_renderer_node(node: latest::RendererNode) -> Vec<u8> {
    node.encode()
}

/// Read back a face kept as [`encode_renderer_node`] wrote it.
#[uniffi::export]
pub fn decode_renderer_node(bytes: Vec<u8>) -> Result<latest::RendererNode, NativeRendererError> {
    latest::RendererNode::decode_with_depth_limit(MAX_FACE_DEPTH, &mut bytes.as_slice()).map_err(
        |error| NativeRendererError::Malformed {
            reason: error.to_string(),
        },
    )
}

/// Screen a product-supplied Pocket card id with the rules every Pocket call
/// already applies, so a card declared in a manifest is refused where it is
/// declared rather than at its first wire call.
///
/// Hosts call this instead of screening ids themselves: the rules are the
/// core's, and a host that guesses at them refuses links the core accepted.
#[uniffi::export]
pub fn screen_pocket_card_id(id: String) -> Result<String, NativeChatFieldError> {
    crate::platform::normalize_chat_identifier("cardId", &id).map_err(Into::into)
}

/// Screen the display title a product gives one of its cards.
///
/// A title is drawn, not addressed, so it carries the display rules rather than
/// the stricter identifier ones. Emoji and Persian need the joiners and
/// variation selectors an identifier refuses, and a title screened as an id
/// would cost a product every card it publishes.
#[uniffi::export]
pub fn screen_pocket_card_title(title: String) -> Result<String, NativeChatFieldError> {
    crate::platform::validate_chat_name("title", &title).map_err(Into::into)
}

/// Whether `product_id` is a first-party product the host grants every
/// [`truapi::latest::RemotePermission`] without prompting.
///
/// Blessed products bypass recorded permissions. Only device permissions require
/// consent. Hosts mediating product network access can use this check before storage.
///
/// Normalizes before matching, and answers `false` for an id that does not
/// normalize, so an unknown spelling is never read as trusted.
#[uniffi::export]
pub fn has_trusted_remote_permissions(product_id: String) -> bool {
    crate::platform::normalizes_to_trusted_remote_permissions(&product_id)
}

/// Set the live log level (`off`/`error`/`warn`/`info`/`debug`/`trace`) for
/// the `tracing` output, which on native routes to stderr (system logs on
/// iOS/Android). Most native diagnostics flow through `on_core_log` instead;
/// this controls the cross-platform `tracing` events shared with wasm.
#[uniffi::export]
pub fn set_log_level(level: String) {
    crate::logging::set_level_from_str(&level);
}

#[cfg(test)]
mod tests;
