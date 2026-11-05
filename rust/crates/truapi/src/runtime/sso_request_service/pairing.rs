//! SSO pairing (login): presents the pairing deeplink, watches the bootstrap
//! topic on the statement store (live subscription plus periodic snapshot
//! queries), and decrypts the wallet's V2 handshake response into a session.

use core::pin::Pin;

use futures::future::Fuse;

#[cfg(test)]
use std::sync::{Arc, Mutex};

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;
#[cfg(target_arch = "wasm32")]
use web_time::Duration;

use super::{
	super::{
		auth_state::AuthStateMachine, identity::resolve_session_identity_with_chain,
		statement_store_rpc,
	},
	SsoRequestService,
};
use crate::{
	host_logic::{
		session::{SessionInfo, encode_persisted_session},
		sso::pairing::{
			PairingBootstrap, PairingDeviceIdentity, VersionedHandshakeResponse,
			create_pairing_bootstrap_from_identity, decode_app_handshake_data,
			decrypt_v2_handshake_response, establish_sso_session_info,
			generate_pairing_device_identity, v2,
		},
		statement_store::{decode_verified_statement_data, parse_new_statements_result},
	},
	subscription::Spawner,
};

use crate::platform::{CoreStorage, CoreStorageKey};
use futures::{
	FutureExt, StreamExt,
	channel::{mpsc, oneshot},
	pin_mut,
};
use parity_scale_codec::{Decode, Encode};
use serde_json::Value;
use subxt_rpcs::{RpcClient, client::RpcSubscription};
use tracing::{debug, info, instrument};
#[cfg(test)]
use truapi::versioned::account::HostRequestLoginResponse;
use truapi::{CallError, latest as api, latest::HostRequestLoginError};

#[cfg(not(test))]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_secs(2);
#[cfg(test)]
const PAIRING_QUERY_INTERVAL: Duration = Duration::from_millis(1);
#[cfg(not(test))]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 15;
#[cfg(test)]
const PAIRING_QUERY_TIMEOUT_TICKS: u8 = 10;

// Bound silent handshakes, including storage and RPC setup before the first answer.
#[cfg(not(test))]
const PAIRING_DEADLINE: Duration = Duration::from_secs(300);
#[cfg(test)]
const PAIRING_DEADLINE: Duration = Duration::from_millis(200);

fn pairing_deadline_reason() -> String {
	let seconds = PAIRING_DEADLINE.as_secs();
	format!(
		"pairing did not complete within {seconds}s; the peer may be answering on a different SSO envelope"
	)
}

/// Terminal outcome of [`SsoPairingFlow::request_session`].
pub enum SsoPairingOutcome {
	/// Pairing was cancelled or another activation won.
	Cancelled,
	/// Wallet handshake completed; the session is resolved and persisted.
	Success(Box<SessionInfo>),
}

// Dropped requests must not leave their pairing UI active.
struct AbandonedPairingGuard {
	auth_state: AuthStateMachine,
	epoch: u64,
	active: bool,
}

impl AbandonedPairingGuard {
	fn disarm(&mut self) {
		self.active = false;
	}
}

impl Drop for AbandonedPairingGuard {
	fn drop(&mut self) {
		if self.active {
			self.auth_state.reset_abandoned_pairing(self.epoch);
		}
	}
}

/// One pairing (login) attempt driven on behalf of a pairing host.
pub struct SsoPairingFlow<'a> {
	host: &'a SsoRequestService,
}

impl<'a> SsoPairingFlow<'a> {
	/// Bind a pairing attempt to its host.
	pub fn new(host: &'a SsoRequestService) -> Self {
		Self { host }
	}

	/// Run pairing and persist the resolved wallet session.
	pub async fn request_session(
		&self,
	) -> Result<SsoPairingOutcome, CallError<HostRequestLoginError>> {
		// Armed here rather than inside the flow because the reads below are
		// host callbacks: a platform whose storage never answers would otherwise
		// hold the attempt before the flow that bounds it ever starts, which is
		// the silence this deadline exists to end.
		let deadline = futures_timer::Delay::new(PAIRING_DEADLINE).fuse();
		pin_mut!(deadline);

		let prepare = async {
			let (mut pairing_identity, reused_identity) =
				read_or_create_pairing_device_identity(self.host.platform.as_ref()).await?;
			let last_processed_statement =
				read_last_processed_pairing_statement(self.host.platform.as_ref()).await?;
			// Pairing success statements are retained by statement-store. Reusing a
			// previous pairing identity means reusing its topic, where the only
			// retained response may be the last processed success. Rotate before
			// presenting QR so every explicit login waits on a fresh wallet scan.
			if reused_identity {
				debug!("regenerating stored pairing device identity");
				pairing_identity =
					create_fresh_pairing_device_identity(self.host.platform.as_ref()).await?;
			}
			let bootstrap =
				create_pairing_bootstrap_from_identity(&self.host.host_config, pairing_identity)
					.map_err(|err| err.to_string())?;
			Ok::<_, String>((bootstrap, last_processed_statement))
		}
		.fuse();
		pin_mut!(prepare);

		let (bootstrap, last_processed_statement) = futures::select! {
			_ = deadline => return Err(self.fail_before_pairing(pairing_deadline_reason())),
			prepared = prepare => prepared.map_err(|reason| self.fail_before_pairing(reason))?,
		};

		let Some((cancel_rx, pairing_epoch)) =
			self.host.auth_state.pairing_started(bootstrap.deeplink.clone())
		else {
			return Err(CallError::Domain(api::HostRequestLoginError::Unknown {
				reason: "login already in progress".to_string(),
			}));
		};
		info!("presenting pairing QR, waiting for wallet handshake");
		let mut reset_guard = AbandonedPairingGuard {
			auth_state: self.host.auth_state.clone(),
			epoch: pairing_epoch,
			active: true,
		};

		match self
			.run_pairing_flow(
				&bootstrap,
				cancel_rx,
				pairing_epoch,
				last_processed_statement,
				deadline,
			)
			.await
		{
			Ok(outcome @ SsoPairingOutcome::Cancelled) => {
				reset_guard.disarm();
				Ok(outcome)
			},
			Ok(outcome @ SsoPairingOutcome::Success(_)) => {
				reset_guard.disarm();
				Ok(outcome)
			},
			Err(reason) => {
				self.host.auth_state.login_failed(reason.clone());
				Err(CallError::HostFailure { reason })
			},
		}
	}

	fn fail_before_pairing(&self, reason: String) -> CallError<HostRequestLoginError> {
		self.host.auth_state.login_failed_before_pairing(reason.clone());
		CallError::Domain(api::HostRequestLoginError::Unknown { reason })
	}

	async fn run_pairing_flow(
		&self,
		bootstrap: &PairingBootstrap,
		cancel_rx: oneshot::Receiver<()>,
		pairing_epoch: u64,
		last_processed_statement: Option<Vec<u8>>,
		mut deadline: Pin<&mut Fuse<futures_timer::Delay>>,
	) -> Result<SsoPairingOutcome, String> {
		let mut cancel = cancel_rx.fuse();
		let statement_store = self.host.statement_store.clone();
		let statement_store_connect = statement_store.client("pairing statement-store").fuse();
		pin_mut!(statement_store_connect);

		let rpc_client = futures::select! {
			_ = cancel => return Ok(SsoPairingOutcome::Cancelled),
			_ = deadline => return Err(pairing_deadline_reason()),
			connect_result = statement_store_connect => connect_result.map_err(|err| err.to_string())?,
		};
		let subscribe_client = rpc_client.clone();
		let live_topics = [bootstrap.topic];
		let live_subscription =
			statement_store_rpc::subscribe_match_all(&subscribe_client, &live_topics).fuse();
		pin_mut!(live_subscription);
		let live_subscription = futures::select! {
			_ = cancel => return Ok(SsoPairingOutcome::Cancelled),
			_ = deadline => return Err(pairing_deadline_reason()),
			subscribe_result = live_subscription => subscribe_result
				.map_err(|err| format!("pairing statement-store subscribe failed: {err}"))?,
		};
		debug!("subscribed to pairing topic, polling statement store");
		let pairing_response = wait_for_v2_pairing_success(
			rpc_client,
			live_subscription,
			bootstrap.topic,
			bootstrap.encryption_secret_key,
			last_processed_statement,
			PairingProgressObserver { auth_state: &self.host.auth_state, pairing_epoch },
			self.host.spawner.clone(),
		)
		.fuse();
		pin_mut!(pairing_response);

		let response = futures::select! {
			_ = cancel => return Ok(SsoPairingOutcome::Cancelled),
			_ = deadline => return Err(pairing_deadline_reason()),
			response_result = pairing_response => response_result?,
		};
		write_last_processed_pairing_statement(self.host.platform.as_ref(), &response.statement)
			.await;
		let sso = establish_sso_session_info(
			bootstrap,
			response.peer_statement_account_id,
			response.success.sso_enc_pub_key,
		)?;
		let session = SessionInfo {
			public_key: response.success.root_account_id,
			sso: Some(sso),
			root_entropy_source: Some(response.success.root_entropy_source),
			identity_account_id: Some(response.success.identity_account_id),
			identity_chat_private_key: Some(response.success.identity_chat_private_key),
			device_enc_public_key: Some(response.success.device_enc_pub_key),
			lite_username: None,
			full_username: None,
		};
		let resolve_session = resolve_session_identity_with_chain(
			&self.host.chain,
			self.host.host_config.asset_hub_chain_genesis_hash,
			session,
		)
		.fuse();
		pin_mut!(resolve_session);
		let session = futures::select! {
			_ = cancel => return Ok(SsoPairingOutcome::Cancelled),
			session = resolve_session => session,
		};
		let persist_session = self
			.host
			.platform
			.write_core_storage(CoreStorageKey::AuthSession, encode_persisted_session(&session))
			.fuse();
		pin_mut!(persist_session);
		futures::select! {
			_ = cancel => {
				clear_auth_session(self.host.platform.as_ref()).await;
				return Ok(SsoPairingOutcome::Cancelled);
			},
			persist_result = persist_session => persist_result
				.map_err(|err| format!("session persist failed: {err:?}"))?,
		};
		futures::select! {
			_ = cancel => {
				clear_auth_session(self.host.platform.as_ref()).await;
				return Ok(SsoPairingOutcome::Cancelled);
			},
			default => {}
		};
		Ok(SsoPairingOutcome::Success(Box::new(session)))
	}
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing_device.create_fresh"))]
async fn create_fresh_pairing_device_identity(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<PairingDeviceIdentity, String> {
	let identity = generate_pairing_device_identity()
		.map_err(|err| format!("pairing identity failed: {err}"))?;
	storage
		.write_core_storage(CoreStorageKey::PairingDeviceIdentity, identity.encode())
		.await
		.map_err(|err| format!("pairing device identity write failed: {err:?}"))?;
	Ok(identity)
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing_device.read_or_create"))]
async fn read_or_create_pairing_device_identity(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<(PairingDeviceIdentity, bool), String> {
	let stored = storage
		.read_core_storage(CoreStorageKey::PairingDeviceIdentity)
		.await
		.map_err(|err| format!("pairing device identity read failed: {err:?}"))?;
	if let Some(stored) = stored {
		match PairingDeviceIdentity::decode(&mut stored.as_slice()) {
			Ok(identity) => return Ok((identity, true)),
			Err(err) => {
				debug!("discarding invalid stored pairing device identity: {err}");
			},
		}
	}

	create_fresh_pairing_device_identity(storage)
		.await
		.map(|identity| (identity, false))
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.last_processed.read"))]
async fn read_last_processed_pairing_statement(
	storage: &(impl CoreStorage + ?Sized),
) -> Result<Option<Vec<u8>>, String> {
	storage
		.read_core_storage(CoreStorageKey::LastProcessedPairingStatement)
		.await
		.map_err(|err| format!("last processed pairing statement read failed: {err:?}"))
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.last_processed.write"))]
async fn write_last_processed_pairing_statement(
	storage: &(impl CoreStorage + ?Sized),
	statement: &[u8],
) {
	if let Err(err) = storage
		.write_core_storage(CoreStorageKey::LastProcessedPairingStatement, statement.to_vec())
		.await
	{
		debug!("last processed pairing statement write failed: {err:?}");
	}
}

#[instrument(skip_all, fields(runtime.method = "sso.auth_session.clear"))]
async fn clear_auth_session(storage: &(impl CoreStorage + ?Sized)) {
	if let Err(err) = storage.clear_core_storage(CoreStorageKey::AuthSession).await {
		debug!("auth session clear failed: {err:?}");
	}
}

/// Decoded wallet handshake success plus the statement metadata needed to
/// persist the authenticated session and remember the handled statement.
struct PairingSuccess {
	statement: Vec<u8>,
	peer_statement_account_id: [u8; 32],
	success: v2::Success,
}

enum PairingProgress {
	Pending,
	Success(Box<PairingSuccess>),
}

impl PairingProgress {
	/// Decode one retained statement-store response for the current pairing
	/// topic. Authenticated pending statuses remain non-terminal; wallet
	/// failure statuses are surfaced as `Err`.
	#[instrument(skip_all, fields(runtime.method = "sso.pairing.decode_statement"))]
	fn from_v2_statement(
		statement: &[u8],
		core_encryption_secret_key: [u8; 32],
	) -> Result<Self, String> {
		let verified =
			decode_verified_statement_data(statement, None).map_err(|err| err.to_string())?;
		let VersionedHandshakeResponse::V2 { encrypted_message, public_key } =
			decode_app_handshake_data(&verified.data)?;
		match decrypt_v2_handshake_response(
			core_encryption_secret_key,
			public_key,
			&encrypted_message,
		)? {
			v2::EncryptedResponse::Pending(_) => Ok(Self::Pending),
			v2::EncryptedResponse::Failed(reason) => Err(reason),
			v2::EncryptedResponse::Success(success) => {
				Ok(Self::Success(Box::new(PairingSuccess {
					statement: statement.to_vec(),
					peer_statement_account_id: verified.signer,
					success: *success,
				})))
			},
		}
	}
}

struct PairingProgressObserver<'a> {
	auth_state: &'a AuthStateMachine,
	pairing_epoch: u64,
}

impl PairingProgressObserver<'_> {
	fn observe(&self, progress: PairingProgress) -> Option<PairingSuccess> {
		self.auth_state.authentication_started(self.pairing_epoch);
		match progress {
			PairingProgress::Pending => None,
			PairingProgress::Success(success) => Some(*success),
		}
	}
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.wait_success"))]
async fn wait_for_v2_pairing_success(
	rpc_client: RpcClient,
	mut live_subscription: RpcSubscription<Value>,
	topic: [u8; 32],
	core_encryption_secret_key: [u8; 32],
	last_processed_statement: Option<Vec<u8>>,
	progress_observer: PairingProgressObserver<'_>,
	spawner: Spawner,
) -> Result<PairingSuccess, String> {
	let (query_tx, mut query_rx) = mpsc::unbounded();
	let mut query_active = false;
	let poll = futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse();
	pin_mut!(poll);
	loop {
		futures::select! {
			item = live_subscription.next().fuse() => {
				let Some(item) = item else {
					return Err("pairing statement-store live subscription ended".to_string());
				};
				let value = item.map_err(|err| format!("pairing statement-store live error: {err}"))?;
				if let Some(progress) = handle_v2_pairing_result(
					&value,
					core_encryption_secret_key,
					last_processed_statement.as_deref(),
				)?
					&& let Some(success) = progress_observer.observe(progress)
				{
					return Ok(success);
				}
			}
			query = query_rx.next().fuse() => {
				query_active = false;
				if let Some(query) = query
					&& let Some(progress) = query?
					&& let Some(success) = progress_observer.observe(progress)
				{
					return Ok(success);
				}
			}
			_ = poll => {
				if !query_active {
					query_active = true;
					let rpc_client = rpc_client.clone();
					let query_tx = query_tx.clone();
					let last_processed_statement = last_processed_statement.clone();
					let fut = async move {
						let result = run_pairing_snapshot_query(
							rpc_client,
							topic,
							core_encryption_secret_key,
							last_processed_statement,
						).await;
						let _ = query_tx.unbounded_send(result);
					};
					// `RpcClient` is transport-only here; spawning lets live
					// notifications continue to be consumed while a snapshot
					// query is waiting for backlog completion or timeout.
					(spawner)(fut.boxed());
				}
				poll.set(futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse());
			}
		}
	}
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.snapshot_query"))]
async fn run_pairing_snapshot_query(
	rpc_client: RpcClient,
	topic: [u8; 32],
	core_encryption_secret_key: [u8; 32],
	last_processed_statement: Option<Vec<u8>>,
) -> Result<Option<PairingProgress>, String> {
	let topics = [topic];
	let mut subscription = statement_store_rpc::subscribe_match_all(&rpc_client, &topics)
		.await
		.map_err(|err| format!("pairing statement-store query failed: {err}"))?;
	for _ in 0..PAIRING_QUERY_TIMEOUT_TICKS {
		let timeout = futures_timer::Delay::new(PAIRING_QUERY_INTERVAL).fuse();
		pin_mut!(timeout);
		futures::select! {
			item = subscription.next().fuse() => {
				let Some(item) = item else {
					return Ok(None);
				};
				let value = item.map_err(|err| format!("pairing statement-store query item failed: {err}"))?;
				if let Some(progress) = handle_v2_pairing_result(
					&value,
					core_encryption_secret_key,
					last_processed_statement.as_deref(),
				)? {
					return Ok(Some(progress));
				}
				let page = parse_new_statements_result("query".to_string(), &value)
					.map_err(|err| err.to_string())?;
				if page.remaining == Some(0) {
					return Ok(None);
				}
			}
			_ = timeout => {}
		}
	}
	Ok(None)
}

#[instrument(skip_all, fields(runtime.method = "sso.pairing.handle_result"))]
fn handle_v2_pairing_result(
	value: &Value,
	core_encryption_secret_key: [u8; 32],
	last_processed_statement: Option<&[u8]>,
) -> Result<Option<PairingProgress>, String> {
	let page =
		parse_new_statements_result("pairing".to_string(), value).map_err(|err| err.to_string())?;
	let mut pending = false;
	for statement in page.statements {
		if last_processed_statement == Some(statement.as_slice()) {
			continue;
		}
		match PairingProgress::from_v2_statement(&statement, core_encryption_secret_key)? {
			PairingProgress::Pending => pending = true,
			success @ PairingProgress::Success(_) => return Ok(Some(success)),
		}
	}

	Ok(pending.then_some(PairingProgress::Pending))
}

#[cfg(test)]
mod tests;
