//! dotNS identity lookup used to resolve usernames for a session.
//!
//! Usernames live in the dotNS contracts on Asset Hub. The gateway pallet
//! storage anchors the `DotnsPopController`. The protocol registry locates the
//! `StoreFactory`. The account's labels come from its `LabelStore` on the warm
//! path. On the cold path they come from its pending claim on the controller,
//! covering gateway-minted names before the user settles their store.
//!
//! All reads run over one `chainHead_v1` follow via `ReviveApi_call` dry-runs.
//! No chain metadata is needed.

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use crate::{
	chain_runtime::ChainRuntime,
	host_logic::{
		dotns_gateway::{
			DotnsError, DotnsIdentity, classify_labels, discover_pop_controller, resolve_labels,
		},
		session::SessionInfo,
	},
	runtime::dotns_lookup::{DotnsLookup, DotnsLookupKind},
	session_usernames::SessionUsernames,
};

use futures::{FutureExt, pin_mut};
use tracing::{debug, instrument, warn};

/// Budget for the whole username resolution of one session: every attempt for
/// the identity account plus the root-key fallback share it, so a slow or dead
/// endpoint delays session installation by at most this long.
const LOOKUP_BUDGET: Duration = Duration::from_secs(45);

/// Fills in missing usernames by querying the dotNS contracts on Asset Hub.
/// Returns the session unchanged when it already carries a username. Also
/// returns it unchanged when no Asset Hub is configured.
#[instrument(skip_all, fields(runtime.method = "session.identity.resolve_with_chain"))]
pub async fn resolve_session_identity_with_chain(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	session: SessionInfo,
) -> SessionInfo {
	if session.has_username() {
		return session;
	}
	match lookup_session_identity(chain, asset_hub_chain_genesis_hash, &session).await {
		Some(identity) => {
			let (lite_username, full_username) = identity.into_usernames();
			SessionInfo { lite_username, full_username, ..session }
		},
		None => session,
	}
}

/// Reads `session`'s usernames from the dotNS contracts on Asset Hub, whatever
/// usernames it already carries. The identity account is tried first, then the
/// root key when the identity account holds no username. Returns `None` when no
/// Asset Hub is configured, neither account holds a username, or the lookup
/// fails.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup_session"))]
pub async fn lookup_session_identity(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	session: &SessionInfo,
) -> Option<DotnsIdentity> {
	if asset_hub_chain_genesis_hash == [0; 32] {
		return None;
	}

	let budget = futures_timer::Delay::new(LOOKUP_BUDGET).fuse();
	pin_mut!(budget);
	let lookup = async {
		let preferred_account = session.identity_account_id.unwrap_or(session.public_key);
		match lookup_with_retries(
			chain,
			asset_hub_chain_genesis_hash,
			preferred_account,
			"identity",
		)
		.await
		{
			LookupOutcome::Found(identity) => Some(identity),
			LookupOutcome::NoRecord if preferred_account != session.public_key => {
				match lookup_with_retries(
					chain,
					asset_hub_chain_genesis_hash,
					session.public_key,
					"root identity",
				)
				.await
				{
					LookupOutcome::Found(identity) => Some(identity),
					LookupOutcome::NoRecord | LookupOutcome::Failed => None,
				}
			},
			LookupOutcome::NoRecord | LookupOutcome::Failed => None,
		}
	}
	.fuse();
	pin_mut!(lookup);
	futures::select! {
		identity = lookup => identity,
		() = budget => {
			warn!(
				"dotNS username resolution ran out of budget; the session installs without one"
			);
			None
		}
	}
}

/// Maximum lookup attempts per account on transient failure. The first attempt
/// warms the Asset Hub connection, cached per genesis. A retry after a cold-start
/// timeout therefore usually resolves immediately.
const IDENTITY_LOOKUP_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
enum LookupOutcome {
	/// A username record was found.
	Found(DotnsIdentity),
	/// The account has no dotNS username. Definitive, not worth a retry.
	NoRecord,
	/// The lookup failed transiently after exhausting retries.
	Failed,
}

/// Looks up `account`'s dotNS identity, retrying transient failures against the
/// warmed connection.
async fn lookup_with_retries(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	account: [u8; 32],
	label: &str,
) -> LookupOutcome {
	for attempt in 1..=IDENTITY_LOOKUP_MAX_ATTEMPTS {
		match lookup_dotns_identity(chain, asset_hub_chain_genesis_hash, account).await {
			Ok(Some(identity)) => {
				debug!(
					account = %hex::encode(account),
					lite_username = ?identity.lite_username(),
					full_username = ?identity.full_username(),
					"dotNS {label} lookup found username"
				);
				return LookupOutcome::Found(identity);
			},
			Ok(None) => {
				debug!(
					account = %hex::encode(account),
					"dotNS {label} lookup found no username"
				);
				return LookupOutcome::NoRecord;
			},
			Err(reason) => {
				// A warning leaves the device, so the account is named by a
				// handle rather than in full. The debug lines above, which stay
				// on it, still carry the whole id.
				warn!(
					account = %crate::logging::account_handle(&account),
					attempt,
					reason = %reason.reason(),
					"dotNS {label} lookup failed"
				);
			},
		}
	}
	LookupOutcome::Failed
}

/// Resolves `account_id`'s usernames from the dotNS contracts at a fresh Asset
/// Hub head. Each step carries the lookup transport's own step timeout; the caller's
/// [`LOOKUP_BUDGET`] bounds the whole resolution. Returns `None` when the
/// gateway is not deployed. Also returns `None` when none of the account's
/// labels is a username.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup"))]
async fn lookup_dotns_identity(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	account_id: [u8; 32],
) -> Result<Option<DotnsIdentity>, DotnsError> {
	let lookup = async {
		let mut lookup = DotnsLookup::pinned_to_best_block(
			chain,
			asset_hub_chain_genesis_hash,
			DotnsLookupKind::Identity,
		)
		.await?;
		let Some(controller) = discover_pop_controller(&mut lookup).await? else {
			return Ok(None);
		};
		let labels = resolve_labels(&mut lookup, &controller, &account_id).await?;
		classify_labels(&mut lookup, &controller, labels).await
	}
	.fuse();
	pin_mut!(lookup);
	lookup.await
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
