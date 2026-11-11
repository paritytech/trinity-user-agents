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
			DotnsError, DotnsIdentity, DotnsTransport, classify_labels, discover_pop_controller,
			resolve_labels,
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

/// Fills in missing usernames before installing a session.
/// Cached usernames are retained. Unavailable discovery, transport, or budget
/// leaves the session unchanged; disclosure callers must instead use
/// [`lookup_session_identity`] and propagate its errors.
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
		Ok(Some(identity)) => {
			let (lite_username, full_username) = identity.into_usernames();
			SessionInfo { lite_username, full_username, ..session }
		},
		Ok(None) => session,
		Err(reason) => {
			warn!(%reason, "dotNS username resolution unavailable; installing unchanged session");
			session
		},
	}
}

/// Reads usernames regardless of the session's cached names.
/// Only confirmed absence of issued usernames permits the root-key fallback.
/// `Ok(None)` means neither account has a username; missing configuration,
/// gateway discovery, failed reads, and budget exhaustion remain errors.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup_session"))]
pub async fn lookup_session_identity(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	session: &SessionInfo,
) -> Result<Option<DotnsIdentity>, String> {
	if asset_hub_chain_genesis_hash == [0; 32] {
		return Err("dotNS username lookup unavailable: no Asset Hub configured".to_string());
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
		.await?
		{
			None if preferred_account != session.public_key => {
				lookup_with_retries(
					chain,
					asset_hub_chain_genesis_hash,
					session.public_key,
					"root identity",
				)
				.await
			},
			identity => Ok(identity),
		}
	}
	.fuse();
	pin_mut!(lookup);
	futures::select! {
		identity = lookup => identity,
		() = budget => {
			Err("dotNS username lookup unavailable: resolution exceeded 45-second budget".to_string())
		}
	}
}

/// Maximum lookup attempts per account on transient failure. The first attempt
/// warms the Asset Hub connection, cached per genesis. A retry after a cold-start
/// timeout therefore usually resolves immediately.
const IDENTITY_LOOKUP_MAX_ATTEMPTS: usize = 3;

enum IdentityLookupError {
	/// Discovery succeeded, but the required gateway is not deployed.
	Unavailable(&'static str),
	/// Preserve bounded retries for failed chain reads.
	Failed(Box<DotnsError>),
}

impl From<DotnsError> for IdentityLookupError {
	fn from(reason: DotnsError) -> Self {
		Self::Failed(Box::new(reason))
	}
}

/// Looks up an account, retrying failed reads but not confirmed absence or a
/// missing gateway. No partial lookup result is applied to the session.
async fn lookup_with_retries(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	account: [u8; 32],
	label: &str,
) -> Result<Option<DotnsIdentity>, String> {
	for attempt in 1..=IDENTITY_LOOKUP_MAX_ATTEMPTS {
		match lookup_dotns_identity(chain, asset_hub_chain_genesis_hash, account).await {
			Ok(Some(identity)) => {
				debug!(
					account = %crate::logging::Account::new(&account).in_full(),
					lite_username = ?identity.lite_username(),
					full_username = ?identity.full_username(),
					"dotNS {label} lookup found username"
				);
				return Ok(Some(identity));
			},
			Ok(None) => {
				debug!(
					account = %crate::logging::Account::new(&account).in_full(),
					"dotNS {label} lookup found no username"
				);
				return Ok(None);
			},
			Err(IdentityLookupError::Unavailable(reason)) => {
				return Err(format!("dotNS {label} username lookup unavailable: {reason}"));
			},
			Err(IdentityLookupError::Failed(reason)) => {
				warn!(
					account = %crate::logging::Account::new(&account),
					attempt,
					reason = %reason.reason(),
					"dotNS {label} lookup failed"
				);
				if attempt == IDENTITY_LOOKUP_MAX_ATTEMPTS {
					return Err(format!(
						"dotNS {label} username lookup unavailable after {attempt} attempts: {reason}"
					));
				}
			},
		}
	}
	unreachable!("identity lookup always attempts at least once")
}

/// Resolves `account_id`'s usernames from the dotNS contracts at a fresh Asset
/// Hub head. Each step carries the lookup transport's own step timeout; the caller's
/// [`LOOKUP_BUDGET`] bounds the whole resolution. Returns `None` when no label
/// becomes an issued username; absent gateway discovery is unavailable.
#[instrument(skip_all, fields(runtime.method = "session.identity.lookup"))]
async fn lookup_dotns_identity(
	chain: &ChainRuntime,
	asset_hub_chain_genesis_hash: [u8; 32],
	account_id: [u8; 32],
) -> Result<Option<DotnsIdentity>, IdentityLookupError> {
	let mut lookup = DotnsLookup::pinned_to_best_block(
		chain,
		asset_hub_chain_genesis_hash,
		DotnsLookupKind::Identity,
	)
	.await?;
	let Some(controller) = discover_pop_controller(&mut lookup).await? else {
		return Err(IdentityLookupError::Unavailable(
			"dotNS gateway is not deployed on the configured Asset Hub",
		));
	};
	let labels = resolve_labels(&mut lookup, &controller, &account_id).await?;
	Ok(classify_labels(&mut lookup, &controller, labels).await?)
}

/// Resolve the account's Lite identity, requiring gateway ownership as well as PoP provenance.
pub(super) async fn lookup_local_identity(
	chain: &ChainRuntime,
	genesis: [u8; 32],
	account: [u8; 32],
) -> Result<Option<DotnsIdentity>, String> {
	let operation = async {
		let mut lookup =
			DotnsLookup::pinned_to_best_block(chain, genesis, DotnsLookupKind::Identity)
				.await
				.map_err(|error| error.reason())?;
		let controller = discover_pop_controller(&mut lookup)
			.await
			.map_err(|error| error.reason())?
			.ok_or("dotNS gateway is not deployed on the configured Asset Hub")?;
		let labels = resolve_labels(&mut lookup, &controller, &account)
			.await
			.map_err(|error| error.reason())?;
		let mut owned = Vec::new();
		for label in labels {
			if !crate::host_logic::dotns_gateway::is_dotted_lite_username(&label) {
				continue;
			}
			let owner = lookup
				.storage(crate::host_logic::dotns_gateway::lite_label_owner_key(label.as_bytes()))
				.await
				.map_err(|error| error.reason())?;
			let Some(owner) = owner else { continue };
			let owner: [u8; 32] = owner
				.as_slice()
				.try_into()
				.map_err(|_| "DotnsGateway.LiteLabelOwner is not a 32-byte account")?;
			if owner == account {
				owned.push(label);
			}
		}
		classify_labels(&mut lookup, &controller, owned)
			.await
			.map_err(|error| error.reason())
	}
	.fuse();
	let timeout = futures_timer::Delay::new(LOOKUP_BUDGET).fuse();
	pin_mut!(operation, timeout);
	futures::select! {
		result = operation => result,
		() = timeout => Err("dotNS identity lookup timed out".to_string()),
	}
}

/// Read the reservation timestamp from Asset Hub rather than the browser clock.
pub(super) async fn registration_timestamp(
	chain: &ChainRuntime,
	genesis: [u8; 32],
	_account: [u8; 32],
) -> Result<u64, String> {
	let mut lookup = DotnsLookup::pinned_to_best_block(chain, genesis, DotnsLookupKind::Identity)
		.await
		.map_err(|error| error.reason())?;
	let value = lookup
		.storage(crate::host_logic::dotns_gateway::timestamp_now_key())
		.await
		.map_err(|error| error.reason())?
		.ok_or("Timestamp.Now is unset")?;
	let millis: [u8; 8] = value.as_slice().try_into().map_err(|_| "Timestamp.Now is not a u64")?;
	Ok(u64::from_le_bytes(millis) / 1000)
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests;
