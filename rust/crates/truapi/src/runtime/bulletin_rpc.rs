//! In-core Bulletin preimage submission over the shared Subxt client.
//!
//! One submission at a time: build + sign the `TransactionStorage.store`
//! extrinsic through the core's `ExtrinsicService`, mortal from the finalized
//! head, dry-run it at a best block no older than that head via Subxt's typed
//! transaction validation, then submit through Subxt's transaction watch and
//! classify the dispatch outcome from the inclusion block's events.

use std::sync::{Arc, Mutex as StdMutex};
#[cfg(not(target_arch = "wasm32"))]
use std::time::{Duration, Instant};
#[cfg(target_arch = "wasm32")]
use web_time::{Duration, Instant};

use crate::{
	chain::{ExtrinsicBuildError, ExtrinsicService},
	chain_runtime::{ChainRuntime, RuntimeFailure},
	host_internal::{
		bulletin::{
			STORE_PALLET_NAME, allowance_signer, build_signed_store_transaction, preimage_key,
		},
		extrinsic::Sr25519Signer,
	},
	runtime::BulletinAllowanceKey,
};
use futures::{FutureExt, pin_mut};
use subxt::{
	OnlineClient,
	client::{Block, Blocks, OnlineClientAtBlockImpl},
	config::{HashFor, substrate::SubstrateConfig},
	error::{
		DispatchError, TransactionEventsError, TransactionProgressError, TransactionStatusError,
	},
	extrinsics::ExtrinsicEvents,
	metadata::ArcMetadata,
	tx::{
		SubmittableTransaction, TransactionInBlock, TransactionInvalid, TransactionStatus,
		TransactionUnknown, ValidationResult,
	},
	utils::H256,
};
use tracing::{instrument, warn};
use truapi::CallContext;

/// Rebuild, re-sign, and retry once when the node definitively rejects a
/// broadcast (`invalid`/`dropped`): nothing was included, so a fresh signing
/// cannot double-store.
const SUBMIT_ATTEMPTS: usize = 2;
/// Times the same signed bytes are broadcast at most: the initial submission
/// plus one re-broadcast after a watch that ended without a verdict. The
/// identical transaction can be included at most once, so re-broadcasting it
/// can never double-store the preimage.
const MAX_IDENTICAL_BROADCASTS: usize = 2;
/// Wall-clock window to keep re-checking whether a freshly-claimed allowance
/// has propagated to the dry-run view before treating the rejection as real.
/// Bounding by elapsed time rather than a best-block count keeps the intended
/// ~18s budget stable across changes in Bulletin's block cadence (a fixed
/// count silently shrank the budget when block time dropped), and leaves room
/// in the end-to-end submit timeout for one refresh and retry.
const ALLOWANCE_DRY_RUN_PROPAGATION_WINDOW: Duration = Duration::from_secs(18);
/// Bound each wait for a newer best block while allowance state propagates.
const ALLOWANCE_DRY_RUN_BLOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
/// Finalized blocks to inspect for the already-broadcast transaction after
/// the watch is interrupted (for example when `chainHead_follow` emits a
/// `stop` event mid-submission).
const INCLUSION_RECHECK_BLOCKS: usize = 3;
/// Bound each wait for the next finalized block during the inclusion
/// re-check.
#[cfg(not(test))]
const INCLUSION_RECHECK_BLOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(test)]
const INCLUSION_RECHECK_BLOCK_WAIT_TIMEOUT: Duration = Duration::from_millis(25);
/// Bound on each wait for the next block while picking the head: the first
/// replayed block, and each block read on until the head reaches finality.
const INITIALIZATION_TIMEOUT: Duration = Duration::from_secs(10);
/// Quiet window after which the newest replayed block is taken as the head.
/// The replay delivers the initialized finalized blocks first, oldest first,
/// and any known best block right after; active chains never need the full
/// window.
#[cfg(not(test))]
const BEST_BLOCK_TIMEOUT: Duration = Duration::from_secs(2);
#[cfg(test)]
const BEST_BLOCK_TIMEOUT: Duration = Duration::from_millis(10);

/// `TransactionStorage` module errors that mean the allowance account itself
/// was rejected (missing or exhausted authorization), i.e. the one condition
/// where refreshing the allowance key and retrying can help.
const ALLOWANCE_REJECTED_MODULE_ERRORS: &[&str] =
	&["AuthorizationNotFound", "PermanentAllowanceExceeded"];

/// Where a submission currently is, for retry decisions and timeout reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, derive_more::Display)]
pub enum SubmissionPhase {
	#[display("connect")]
	Connect,
	#[display("build")]
	Build,
	#[display("dry-run")]
	DryRun,
	#[display("watch")]
	Watch,
	#[display("events")]
	Events,
}

/// The at-block client every submission step runs against.
type BulletinAtBlock = OnlineClientAtBlockImpl<SubstrateConfig>;

/// A signed store transaction bound to its build block, ready for the
/// dry-run and submission steps.
type SignedStore = SubmittableTransaction<SubstrateConfig, BulletinAtBlock>;

#[derive(Debug, Clone, PartialEq, Eq)]
enum DryRunStatus {
	Valid,
	AllowanceRejected,
}

/// Typed submission failure driving the retry decision at the runtime call
/// site. Wire mapping stays `v01::PreimageSubmitError::Unknown { reason }`.
#[derive(Debug, derive_more::Display, derive_more::Error)]
pub enum BulletinSubmitError {
	/// The host-owned physical connection could not provide a Subxt client.
	#[display("bulletin chain unavailable: {_0}")]
	Host(#[error(source)] RuntimeFailure),
	/// Subxt failed while validating, submitting, or reading events.
	#[display("subxt: {_0}")]
	Subxt(#[error(source)] Box<subxt::Error>),
	/// The store transaction could not be built.
	#[display("building the store transaction: {_0}")]
	Build(#[error(source)] ExtrinsicBuildError),
	/// The allowance key could not be converted to its purpose-scoped signer.
	#[display("invalid allowance key: {_0}")]
	InvalidAllowanceKey(#[error(not(source))] String),
	/// The runtime rejected the transaction for a non-allowance reason.
	#[display("invalid: {_0:?}")]
	InvalidTransaction(#[error(not(source))] TransactionInvalid),
	/// The runtime could not determine transaction validity.
	#[display("unknown transaction validity: {_0:?}")]
	UnknownTransaction(#[error(not(source))] TransactionUnknown),
	/// The allowance account was rejected; refresh + one retry may help.
	#[display("allowance rejected: {phase}")]
	AllowanceRejected { phase: AllowanceRejectionPhase },
	/// The submission budget elapsed; `phase` names the step reached.
	#[display(
        "timeout: {phase}{}",
        if matches!(*phase, SubmissionPhase::Watch | SubmissionPhase::Events) {
            ", inclusion unverified"
        } else {
            ""
        }
    )]
	Timeout { phase: SubmissionPhase },
	/// Inclusion events contained no explicit dispatch outcome.
	#[display("inclusion unverified: included block reported no dispatch outcome")]
	DispatchOutcomeMissing,
	/// The re-broadcast of the already-signed transaction was rejected by
	/// the node, which usually means the first broadcast consumed the nonce
	/// by landing beyond the inclusion re-check's reach. Fail-closed: the
	/// dispatch outcome is unknown.
	#[display("inclusion unverified: re-broadcast rejected: {reason}")]
	RebroadcastRejected {
		#[error(not(source))]
		reason: String,
	},
	/// The Subxt best-block stream ended without reporting an error.
	#[display("Bulletin best-block stream ended")]
	BestBlockStreamEnded,
	/// The calling context was cancelled.
	#[display("cancelled")]
	Cancelled,
}

impl<Error> From<Error> for BulletinSubmitError
where
	subxt::Error: From<Error>,
{
	fn from(error: Error) -> Self {
		Self::Subxt(Box::new(error.into()))
	}
}

impl From<RuntimeFailure> for BulletinSubmitError {
	fn from(error: RuntimeFailure) -> Self {
		Self::Host(error)
	}
}

/// Which stage rejected the allowance account.
#[derive(Debug, Clone, derive_more::Display)]
pub enum AllowanceRejectionPhase {
	#[display("dry-run")]
	DryRun,
	#[display("dispatch")]
	Dispatch,
}

impl BulletinSubmitError {
	/// Post-broadcast failure that leaves the transaction's fate unknown:
	/// the watch died without a verdict (for example `chainHead_follow`
	/// emitted `stop`) or a reported inclusion could not be verified, as
	/// opposed to the node definitively rejecting the transaction.
	fn is_broadcast_outcome_unknown(&self, phase: SubmissionPhase) -> bool {
		match self {
			Self::Subxt(_) if phase == SubmissionPhase::Watch => !self.is_node_rejection(),
			Self::Subxt(error) if phase == SubmissionPhase::Events => {
				matches!(
					error.as_ref(),
					subxt::Error::TransactionEventsError(
						TransactionEventsError::CannotFindTransactionInBlock { .. }
					)
				)
			},
			_ => false,
		}
	}

	/// The node definitively rejected the transaction (`invalid` or
	/// `dropped`) instead of losing track of it: nothing was included, so
	/// rebuilding and re-signing at a fresh block cannot double-store.
	fn is_node_rejection(&self) -> bool {
		matches!(
			self,
			Self::Subxt(error) if matches!(
				error.as_ref(),
				subxt::Error::TransactionStatusError(
					TransactionStatusError::Invalid(_) | TransactionStatusError::Dropped(_)
				)
			)
		)
	}

	/// The node reported the transaction invalid mid-watch. On a
	/// re-broadcast this is the expected bounce when the first broadcast
	/// already consumed the nonce.
	fn is_invalid_verdict(&self) -> bool {
		matches!(
			self,
			Self::Subxt(error) if matches!(
				error.as_ref(),
				subxt::Error::TransactionStatusError(TransactionStatusError::Invalid(_))
			)
		)
	}
}

/// Bulletin-chain submission service shared by all product runtimes.
pub struct BulletinRpc {
	chain: ChainRuntime,
	extrinsics: Arc<ExtrinsicService>,
	genesis_hash: [u8; 32],
	/// Serializes submissions: no same-account nonce races between
	/// concurrent submits.
	submit_lock: futures::lock::Mutex<()>,
	/// Last phase entered, for timeout reasons.
	phase: StdMutex<SubmissionPhase>,
	/// Wall-clock budget for allowance propagation to reach the dry-run view.
	allowance_propagation_window: Duration,
	/// Bound on each wait for a newer best block while allowance state
	/// propagates.
	allowance_block_wait_timeout: Duration,
}

impl BulletinRpc {
	/// Build a bulletin submission service over the shared chain runtime,
	/// building its transactions through `extrinsics`.
	pub fn new(
		chain: ChainRuntime,
		extrinsics: Arc<ExtrinsicService>,
		genesis_hash: [u8; 32],
	) -> Self {
		Self {
			chain,
			extrinsics,
			genesis_hash,
			submit_lock: futures::lock::Mutex::new(()),
			phase: StdMutex::new(SubmissionPhase::Connect),
			allowance_propagation_window: ALLOWANCE_DRY_RUN_PROPAGATION_WINDOW,
			allowance_block_wait_timeout: ALLOWANCE_DRY_RUN_BLOCK_WAIT_TIMEOUT,
		}
	}

	/// Open a raw RPC client over the configured Bulletin chain.
	pub async fn client(
		&self,
		label: &'static str,
	) -> Result<subxt_rpcs::RpcClient, RuntimeFailure> {
		self.chain
			.rpc_client(label, &self.genesis_hash)
			.await
			.map(subxt_rpcs::RpcClient::new)
	}

	/// Override the allowance-propagation window. Test-only: lets a scripted
	/// run exercise both the "keep polling" and "window elapsed" paths without
	/// depending on real block cadence.
	#[cfg(test)]
	fn with_allowance_propagation_window(mut self, window: Duration) -> Self {
		self.allowance_propagation_window = window;
		self
	}

	/// Override the per-block allowance wait. Test-only: keeps scripted runs
	/// that never produce a newer block from waiting out the real timeout.
	#[cfg(test)]
	fn with_allowance_block_wait_timeout(mut self, timeout: Duration) -> Self {
		self.allowance_block_wait_timeout = timeout;
		self
	}

	/// Submit `value` as a Bulletin preimage signed by `allowance`, returning
	/// the preimage key once the transaction is included and its dispatch
	/// succeeded.
	#[instrument(skip_all, fields(runtime.method = "bulletin_rpc.submit_preimage"))]
	pub async fn submit_preimage(
		&self,
		cx: &CallContext,
		deadline: Instant,
		allowance: &BulletinAllowanceKey,
		value: &[u8],
	) -> Result<Vec<u8>, BulletinSubmitError> {
		// Serialize submissions, keeping the lock wait cancellable.
		let lock = self.submit_lock.lock().fuse();
		let lock_cancelled = cx.cancel().cancelled().fuse();
		pin_mut!(lock, lock_cancelled);
		let _guard = futures::select! {
			guard = lock => guard,
			_ = lock_cancelled => return Err(BulletinSubmitError::Cancelled),
		};

		let mut attempt = 0;
		loop {
			attempt += 1;
			let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
				return Err(BulletinSubmitError::Timeout { phase: self.current_phase() });
			};
			let flow = self.submit_flow(allowance, value).fuse();
			let timeout = futures_timer::Delay::new(remaining).fuse();
			let cancelled = cx.cancel().cancelled().fuse();
			pin_mut!(flow, timeout, cancelled);
			let result = futures::select! {
				result = flow => result,
				() = timeout => Err(BulletinSubmitError::Timeout { phase: self.current_phase() }),
				_ = cancelled => Err(BulletinSubmitError::Cancelled),
			};
			match result {
				Err(err) if attempt < SUBMIT_ATTEMPTS && err.is_node_rejection() => {
					warn!(
						attempt,
						reason = %err,
						"Bulletin broadcast rejected by the node; rebuilding and retrying"
					);
				},
				result => return result,
			}
		}
	}

	async fn submit_flow(
		&self,
		allowance: &BulletinAllowanceKey,
		value: &[u8],
	) -> Result<Vec<u8>, BulletinSubmitError> {
		let key = preimage_key(value);

		self.enter_phase(SubmissionPhase::Connect);
		let client = self.chain.online_client(&self.genesis_hash).await?;
		let finalized = client.at_current_block().await?;
		let mut best_blocks = client.stream_best_blocks().await?;
		let head = initial_best_block(&mut best_blocks, finalized.block_number()).await?;

		self.enter_phase(SubmissionPhase::Build);
		let signer =
			allowance_signer(allowance).map_err(BulletinSubmitError::InvalidAllowanceKey)?;
		let signed = self.build_signed_and_dry_run(&mut best_blocks, head, &signer, value).await?;
		drop(best_blocks);

		self.enter_phase(SubmissionPhase::Watch);
		let mut broadcasts: usize = 1;
		loop {
			let (error, phase) = match watch_until_included(&signed).await {
				Ok(in_block) => {
					self.enter_phase(SubmissionPhase::Events);
					match require_dispatch_success(&in_block).await {
						Ok(()) => break,
						Err(error) => (error, SubmissionPhase::Events),
					}
				},
				Err(error) => (error, SubmissionPhase::Watch),
			};

			// A bounced re-broadcast usually means the first broadcast
			// consumed the nonce, i.e. the transaction landed beyond the
			// re-check's reach.
			let rebroadcast_bounced = broadcasts > 1 && error.is_invalid_verdict();
			if !error.is_broadcast_outcome_unknown(phase) && !rebroadcast_bounced {
				return Err(error);
			}

			// The already-broadcast transaction may be on chain even though
			// the watch lost track of it; prefer its real dispatch outcome
			// over any retry.
			if let Some(outcome) = finalized_inclusion_outcome(&client, &signed).await {
				self.enter_phase(SubmissionPhase::Events);
				outcome?;
				break;
			}

			if rebroadcast_bounced {
				return Err(BulletinSubmitError::RebroadcastRejected { reason: error.to_string() });
			}
			if broadcasts >= MAX_IDENTICAL_BROADCASTS {
				return Err(error);
			}
			broadcasts += 1;
			warn!(
				broadcasts,
				reason = %error,
				"Bulletin watch lost the transaction; re-broadcasting the identical bytes"
			);
			self.enter_phase(SubmissionPhase::Watch);
		}

		Ok(key.to_vec())
	}

	/// Build, sign, and dry-run the extrinsic against the chosen best block.
	/// Dry-run provides the typed signal used to distinguish stale allowances,
	/// nonce races, and other runtime validity failures.
	async fn build_signed_and_dry_run(
		&self,
		best_blocks: &mut Blocks<SubstrateConfig>,
		head: Block<SubstrateConfig>,
		signer: &Sr25519Signer,
		value: &[u8],
	) -> Result<SignedStore, BulletinSubmitError> {
		let mut block = head;
		let mut allowance_rejections = 0;
		let mut allowance_rejection_started = None;
		loop {
			self.enter_phase(SubmissionPhase::Build);
			let built = build_signed_store_transaction(
				&self.extrinsics,
				H256(self.genesis_hash),
				signer,
				value,
			)
			.await
			.map_err(BulletinSubmitError::Build)?;

			self.enter_phase(SubmissionPhase::DryRun);
			let birth = built.mortality.birth().number;
			block = best_block_from(best_blocks, block, birth, SubmissionPhase::DryRun).await?;
			let at_block = block.at().await?;
			let signed = at_block.tx().from_bytes(built.extrinsic.bytes().to_vec());
			let validity = signed.validate().await?;
			match Self::classify_dry_run_validity(validity)? {
				DryRunStatus::Valid => return Ok(signed),
				DryRunStatus::AllowanceRejected => {
					let started = allowance_rejection_started.get_or_insert_with(Instant::now);
					let elapsed = started.elapsed();
					if elapsed >= self.allowance_propagation_window {
						warn!(
							rejections = allowance_rejections + 1,
							elapsed_ms = elapsed.as_millis(),
							allowance_propagation_window_ms =
								self.allowance_propagation_window.as_millis(),
							stop = "propagation-window",
							"Bulletin allowance remained unavailable to dry-run"
						);
						return Err(BulletinSubmitError::AllowanceRejected {
							phase: AllowanceRejectionPhase::DryRun,
						});
					}
					allowance_rejections += 1;
					warn!(
						attempt = allowance_rejections,
						elapsed_ms = elapsed.as_millis(),
						allowance_propagation_window_ms =
							self.allowance_propagation_window.as_millis(),
						block_wait_timeout_ms = self.allowance_block_wait_timeout.as_millis(),
						"Bulletin allowance not visible to dry-run yet; rebuilding at next block"
					);
					block = match next_best_block(
						best_blocks,
						self.allowance_block_wait_timeout,
						SubmissionPhase::DryRun,
					)
					.await
					{
						Err(BulletinSubmitError::Timeout { .. }) => {
							warn!(
								rejections = allowance_rejections,
								elapsed_ms = started.elapsed().as_millis(),
								stop = "block-wait-timeout",
								"Bulletin allowance remained unavailable to dry-run"
							);
							return Err(BulletinSubmitError::AllowanceRejected {
								phase: AllowanceRejectionPhase::DryRun,
							});
						},
						result => result?,
					};
				},
			}
		}
	}

	fn classify_dry_run_validity(
		validity: ValidationResult,
	) -> Result<DryRunStatus, BulletinSubmitError> {
		match validity {
			ValidationResult::Valid(_) => Ok(DryRunStatus::Valid),
			// Bulletin's fee/authorization transaction extensions report
			// stale or missing allowance through these generic validity
			// variants. The runtime does not publish stable Custom codes, so
			// narrowing Custom would bypass the bounded refresh path.
			ValidationResult::Invalid(
				TransactionInvalid::Payment |
				TransactionInvalid::Custom(_) |
				TransactionInvalid::BadSigner,
			) => Ok(DryRunStatus::AllowanceRejected),
			ValidationResult::Invalid(other) => Err(BulletinSubmitError::InvalidTransaction(other)),
			ValidationResult::Unknown(other) => Err(BulletinSubmitError::UnknownTransaction(other)),
		}
	}

	fn enter_phase(&self, phase: SubmissionPhase) {
		*self.phase.lock().expect("phase slot poisoned") = phase;
	}

	fn current_phase(&self) -> SubmissionPhase {
		*self.phase.lock().expect("phase slot poisoned")
	}
}

/// Take the stream's replayed view of the current chain head: the initialized
/// finalized blocks arrive first, oldest first, followed by the newest known
/// best block. Returns the newest block seen during one bounded
/// [`BEST_BLOCK_TIMEOUT`] replay window, read on until it reaches
/// `finalized_number`. The transaction is anchored at that finalized block, and
/// a head behind it would validate against an earlier birth block and read a
/// stale nonce. The window's deadline is absolute so a continuously advancing
/// best-block stream cannot keep initialization alive forever.
async fn initial_best_block(
	blocks: &mut Blocks<SubstrateConfig>,
	finalized_number: u64,
) -> Result<Block<SubstrateConfig>, BulletinSubmitError> {
	let mut block =
		next_best_block(blocks, INITIALIZATION_TIMEOUT, SubmissionPhase::Connect).await?;
	let replay_deadline = futures_timer::Delay::new(BEST_BLOCK_TIMEOUT).fuse();
	pin_mut!(replay_deadline);
	loop {
		let next = blocks.next().fuse();
		pin_mut!(next);
		futures::select! {
			item = next => match item {
				Some(Ok(newer)) => block = newer,
				Some(Err(error)) => {
					return Err(BulletinSubmitError::Subxt(Box::new(error.into())));
				}
				None => return Err(BulletinSubmitError::BestBlockStreamEnded),
			},
			() = replay_deadline => break,
		}
	}
	best_block_from(blocks, block, finalized_number, SubmissionPhase::Connect).await
}

/// `block`, or the first best block after it numbered at least `number`. A
/// transaction validated at a block before its birth block names a birth the
/// node does not know.
async fn best_block_from(
	blocks: &mut Blocks<SubstrateConfig>,
	mut block: Block<SubstrateConfig>,
	number: u64,
	phase: SubmissionPhase,
) -> Result<Block<SubstrateConfig>, BulletinSubmitError> {
	while block.number() < number {
		block = next_best_block(blocks, INITIALIZATION_TIMEOUT, phase).await?;
	}
	Ok(block)
}

async fn next_best_block(
	blocks: &mut Blocks<SubstrateConfig>,
	timeout: Duration,
	phase: SubmissionPhase,
) -> Result<Block<SubstrateConfig>, BulletinSubmitError> {
	let timeout = futures_timer::Delay::new(timeout).fuse();
	let next = blocks.next().fuse();
	pin_mut!(timeout, next);
	futures::select! {
		block = next => match block {
			Some(Ok(block)) => Ok(block),
			Some(Err(error)) => Err(BulletinSubmitError::Subxt(Box::new(error.into()))),
			None => Err(BulletinSubmitError::BestBlockStreamEnded),
		},
		() = timeout => Err(BulletinSubmitError::Timeout { phase }),
	}
}

/// Submit the signed transaction and watch its progress until it lands in a
/// best or finalized block.
async fn watch_until_included(
	signed: &SignedStore,
) -> Result<TransactionInBlock<SubstrateConfig, BulletinAtBlock>, BulletinSubmitError> {
	let mut progress = signed.submit_and_watch().await?;
	while let Some(status) = progress.next().await {
		let status = status?;
		match status {
			TransactionStatus::InBestBlock(block) | TransactionStatus::InFinalizedBlock(block) => {
				return Ok(block);
			},
			TransactionStatus::Invalid { message } => {
				return Err(BulletinSubmitError::Subxt(Box::new(
					TransactionStatusError::Invalid(message).into(),
				)));
			},
			TransactionStatus::Dropped { message } => {
				return Err(BulletinSubmitError::Subxt(Box::new(
					TransactionStatusError::Dropped(message).into(),
				)));
			},
			TransactionStatus::Error { message } => {
				return Err(BulletinSubmitError::Subxt(Box::new(
					TransactionStatusError::Error(message).into(),
				)));
			},
			TransactionStatus::Validated |
			TransactionStatus::Broadcasted |
			TransactionStatus::NoLongerInBestBlock => {},
		}
	}
	Err(BulletinSubmitError::Subxt(Box::new(
		TransactionProgressError::UnexpectedEndOfTransactionStatusStream.into(),
	)))
}

/// Look for the already-broadcast transaction in the next few finalized
/// blocks once the watch has lost track of it and classify its dispatch
/// outcome from the inclusion block's events.
///
/// `None` means the transaction was not seen (or the scan itself failed);
/// the caller keeps the watch's error and its retry semantics. A found
/// transaction always yields a definitive outcome, never a retry.
///
/// Coverage is bounded by the `chainHead` backend: header/body lookups only
/// work for blocks pinned by the follow subscription, so the scan sees the
/// subscription's initial finalized set plus the next few finalized blocks.
/// An inclusion block that finalized and was superseded before the follow
/// (re)started is unreachable and the transaction is treated as not seen.
async fn finalized_inclusion_outcome(
	client: &OnlineClient<SubstrateConfig>,
	signed: &SignedStore,
) -> Option<Result<(), BulletinSubmitError>> {
	let target_hash = signed.hash();
	let mut blocks = match client.stream_blocks().await {
		Ok(blocks) => blocks,
		Err(error) => {
			warn!(%error, "Bulletin inclusion re-check could not stream finalized blocks");
			return None;
		},
	};
	for _ in 0..INCLUSION_RECHECK_BLOCKS {
		let timeout = futures_timer::Delay::new(INCLUSION_RECHECK_BLOCK_WAIT_TIMEOUT).fuse();
		let next = blocks.next().fuse();
		pin_mut!(timeout, next);
		let block = futures::select! {
			block = next => match block {
				Some(Ok(block)) => block,
				Some(Err(error)) => {
					warn!(%error, "Bulletin inclusion re-check finalized stream failed");
					return None;
				}
				None => return None,
			},
			() = timeout => return None,
		};
		match block_inclusion_outcome(&block, target_hash).await {
			Ok(Some(outcome)) => return Some(outcome),
			Ok(None) => {},
			Err(error) => {
				warn!(%error, "Bulletin inclusion re-check could not inspect a finalized block");
				return None;
			},
		}
	}
	None
}

/// Box any subxt error so scan results stay small on the stack.
fn boxed_subxt_error(error: impl Into<subxt::Error>) -> Box<subxt::Error> {
	Box::new(error.into())
}

/// Search one finalized block for the transaction; when present, classify
/// its dispatch outcome fail-closed, mirroring [`require_dispatch_success`].
async fn block_inclusion_outcome(
	block: &Block<SubstrateConfig>,
	target_hash: HashFor<SubstrateConfig>,
) -> Result<Option<Result<(), BulletinSubmitError>>, Box<subxt::Error>> {
	let at_block = block.at().await.map_err(boxed_subxt_error)?;
	let extrinsics = at_block.extrinsics().fetch().await.map_err(boxed_subxt_error)?;
	for extrinsic in extrinsics.iter() {
		let extrinsic = extrinsic.map_err(boxed_subxt_error)?;
		if extrinsic.hash() != target_hash {
			continue;
		}
		let events = extrinsic.events().await.map_err(boxed_subxt_error)?;
		return Ok(Some(dispatch_outcome_from_events(&events, at_block.metadata())));
	}
	Ok(None)
}

/// Classify a found transaction's dispatch outcome from its events.
/// Failure wins over success and inclusion without an explicit
/// `System.ExtrinsicSuccess` stays unverified, mirroring
/// [`require_dispatch_success`].
fn dispatch_outcome_from_events(
	events: &ExtrinsicEvents<SubstrateConfig>,
	metadata: ArcMetadata,
) -> Result<(), BulletinSubmitError> {
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == "System" && event.event_name() == "ExtrinsicFailed" {
			let dispatch_error = DispatchError::decode_from(event.field_bytes(), metadata.clone())
				.map_err(|error| {
					BulletinSubmitError::Subxt(Box::new(
						TransactionEventsError::CannotDecodeDispatchError {
							error,
							bytes: event.field_bytes().to_vec(),
						}
						.into(),
					))
				})?;
			return Err(classify_dispatch_error(dispatch_error));
		}
	}
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == "System" && event.event_name() == "ExtrinsicSuccess" {
			return Ok(());
		}
	}
	Err(BulletinSubmitError::DispatchOutcomeMissing)
}

/// Require a successful dispatch outcome from the inclusion block's events.
/// Fail-closed: inclusion without an explicit `System.ExtrinsicSuccess` event
/// is reported as unverified, never as success.
async fn require_dispatch_success(
	in_block: &TransactionInBlock<SubstrateConfig, BulletinAtBlock>,
) -> Result<(), BulletinSubmitError> {
	match in_block.wait_for_success().await {
		Ok(events) => {
			for event in events.iter() {
				let event = event?;
				if event.pallet_name() == "System" && event.event_name() == "ExtrinsicSuccess" {
					return Ok(());
				}
			}
			Err(BulletinSubmitError::DispatchOutcomeMissing)
		},
		Err(TransactionEventsError::ExtrinsicFailed(error)) => Err(classify_dispatch_error(error)),
		Err(other) => Err(BulletinSubmitError::Subxt(Box::new(other.into()))),
	}
}

/// Map a dispatch failure to the submission error, singling out the
/// allowance-rejection module errors that a key refresh can fix.
fn classify_dispatch_error(error: DispatchError) -> BulletinSubmitError {
	let DispatchError::Module(module_error) = &error else {
		return BulletinSubmitError::Subxt(Box::new(TransactionEventsError::from(error).into()));
	};
	match module_error.details() {
		Ok(details) => {
			if details.pallet.name() == STORE_PALLET_NAME &&
				ALLOWANCE_REJECTED_MODULE_ERRORS.contains(&details.variant.name.as_str())
			{
				BulletinSubmitError::AllowanceRejected { phase: AllowanceRejectionPhase::Dispatch }
			} else {
				BulletinSubmitError::Subxt(Box::new(TransactionEventsError::from(error).into()))
			}
		},
		Err(_) => BulletinSubmitError::Subxt(Box::new(TransactionEventsError::from(error).into())),
	}
}

#[cfg(test)]
mod tests;
