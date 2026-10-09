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
pub use errors::{HostRejection, NativeCoreDatabaseError};
pub use renderer::{NativeRendererObserver, NativeRendererSubscription};
pub use runtime::{
    NativeAnnouncedPairing, NativePairingError, NativeProductExecution, NativeTrUApiHostRuntime,
};
pub use ws_bridge::{WsBridgeEndpoint, WsBridgeStartError};

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
