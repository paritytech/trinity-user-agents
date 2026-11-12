use super::*;
use crate::latest::{FundingInboundUpdate, FundingOutboundUpdate};

use futures::executor::block_on;
use parity_scale_codec::Encode;

use super::store::{put_session_row, rows};
use crate::{
	host_logic::funding::FundingStage,
	store::{Db, open_memory_core_db},
};

const NOW: u64 = 1_700_000_000_000;
const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

fn ramp() -> ProductId {
	ProductId::for_test("ramp.dot")
}

fn session(intent: &str, opened_at_ms: u64) -> FundingSession {
	FundingSession::new(
		intent.to_string(),
		Some(ProductId::for_test("wallet.dot")),
		FundingDirection::In,
		Some(100),
		opened_at_ms,
	)
}

// Recorded funds waiting out their grace are dropped on time even when
// an earlier retry already fired, and funds being read now set no wake.
#[test]
fn the_next_wake_includes_when_recorded_funds_are_read_again() {
	let registry = FundingRegistry::default();
	registry.lock_resolving().insert([1; 32], NOW + 60_000);
	registry.lock_resolving().insert([2; 32], u64::MAX);

	assert_eq!(registry.next_wake(NOW), Some(60_000));
}

/// A worker link that takes nothing, for tests of the catch-up state.
struct IdleLink;

impl FundingProviderLink for IdleLink {
	fn quote(&self, _ask: FundingQuoteAsk) -> BoxFuture<'static, Option<FundingQuoteAnswer>> {
		Box::pin(async { None })
	}

	fn assign(&self, _session: FundingAssignment) -> BoxFuture<'static, bool> {
		Box::pin(async { false })
	}

	fn cancel(&self, _intent: String) -> BoxFuture<'static, Option<FundingCancelAnswer>> {
		Box::pin(async { None })
	}

	fn settle_cancel(
		&self,
		_intent: String,
		_decision: FundingCancelDecision,
	) -> BoxFuture<'static, bool> {
		Box::pin(async { false })
	}
}

// An unanswered call makes a caught-up link due again, but never starts
// a second catch-up beside a running one nor one before the worker is
// ready: a running one is due again once it ends.
#[test]
fn an_unanswered_call_leaves_a_connecting_or_running_catch_up_alone() {
	let registry = FundingRegistry::default();
	registry.link(&ramp(), Arc::new(IdleLink));
	let state = || registry.lock_links().by_provider[&ramp()].catch_up;
	let set = |catch_up| {
		registry.lock_links().by_provider.get_mut(&ramp()).expect("linked").catch_up = catch_up
	};
	let generation = registry.lock_links().by_provider[&ramp()].generation;

	registry.call_unanswered(&ramp(), generation);
	let connecting = state();
	set(CatchUp::Running);
	registry.call_unanswered(&ramp(), generation);
	let running = state();
	registry.caught_up(&ramp(), generation, true);
	let after_run = matches!(state(), CatchUp::Due { .. });
	set(CatchUp::Done);
	registry.call_unanswered(&ramp(), generation);
	let after_done = matches!(state(), CatchUp::Due { .. });

	assert_eq!(
		(connecting, running, after_run, after_done),
		(CatchUp::Connecting, CatchUp::RunningThenDue, true, true)
	);
}

// A worker the host is stopping keeps its link until its connection
// closes, so a call that failed on it waits for the worker started in
// its place.
#[test]
fn a_link_a_call_failed_on_is_passed_over_for_the_next_connection() {
	let registry = FundingRegistry::default();
	let ramp = ramp();
	let stopping: Arc<dyn FundingProviderLink> = Arc::new(IdleLink);
	registry.link(&ramp, stopping.clone());
	registry.connected(&ramp);
	let mut next = Box::pin(registry.connected_link_besides(&ramp, Some(&stopping)));
	let waited = futures::FutureExt::now_or_never(&mut next).is_none();
	let started: Arc<dyn FundingProviderLink> = Arc::new(IdleLink);
	registry.link(&ramp, started.clone());
	registry.connected(&ramp);

	assert!(waited && Arc::ptr_eq(&block_on(next), &started));
}

// A task dropped before it finishes, as when the runtime shuts its tasks
// down, must not leave its session, funds or worker marked as being
// worked on, or the reconciler would never take them up again.
#[test]
fn work_whose_task_is_dropped_is_taken_up_again() {
	let registry = Arc::new(FundingRegistry::default());
	registry.link(&ramp(), Arc::new(IdleLink));
	let generation = registry.lock_links().by_provider[&ramp()].generation;
	registry.lock_links().by_provider.get_mut(&ramp()).expect("linked").catch_up = CatchUp::Running;
	lock(&registry.following).insert("fs_1".to_string());
	registry.lock_resolving().insert([1; 32], u64::MAX);

	drop(CatchingUp { registry: registry.clone(), provider: ramp(), generation, answered: false });
	drop(Claim {
		registry: registry.clone(),
		set: |registry| &registry.following,
		key: "fs_1".to_string(),
	});
	drop(Resolving { registry: registry.clone(), host_id: [1; 32], next_ms: None });

	assert_eq!(
		(
			matches!(registry.lock_links().by_provider[&ramp()].catch_up, CatchUp::Due { .. }),
			lock(&registry.following).is_empty(),
			registry.lock_resolving().is_empty(),
		),
		(true, true, true)
	);
}

// Every quote asked of a provider that never connects waits for it and
// then gives up; what is left behind is at most the last waiter, not one
// per ask.
#[test]
fn waiting_for_a_provider_that_never_connects_leaves_no_trail() {
	let registry = FundingRegistry::default();
	let ramp = ramp();
	for _ in 0..3 {
		let asked = registry.connected_link_besides(&ramp, None);
		assert!(futures::FutureExt::now_or_never(asked).is_none());
	}

	assert_eq!(registry.lock_links().waiters[&ramp].listeners.lock().len(), 1);
}

// A deadline already past belongs to funds whose call is still in
// flight, which the reconciler skips; waking for it would spin.
#[test]
fn a_past_deadline_of_funds_in_flight_sets_no_wake() {
	let registry = FundingRegistry::default();
	registry.lock_resolving().insert([1; 32], NOW - 1);

	assert_eq!(registry.next_wake(NOW), None);
}

fn registry_on(db: &Db) -> FundingRegistry {
	let registry = FundingRegistry::default();
	assert!(registry.install_db(db.clone()));
	registry
}

fn stored_intents(db: &Db) -> Vec<String> {
	block_on(stored_sessions(db))
		.into_iter()
		.map(|session| session.intent)
		.collect()
}

fn insert(registry: &FundingRegistry, session: FundingSession) {
	block_on(registry.commit(NOW, move |draft| {
		draft.sessions.insert(session.intent.clone(), session);
		((), Vec::new())
	}))
	.expect("inserted");
}

#[test]
fn a_settled_session_survives_later_saves() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	insert(&registry, session("fs_old", NOW - DAY_MS));
	block_on(registry.sweep(NOW)).expect("swept");

	insert(&registry, session("fs_new", NOW));

	assert_eq!(
		(stored_intents(&db), block_on(rows(&db, "funding_summary")).len()),
		(vec!["fs_new".to_string()], 1)
	);
}

#[test]
fn a_subscriber_sees_the_current_stage_then_expiry_and_the_end() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	insert(&registry, session("fs_1", NOW));
	let stream = registry.subscribe("fs_1").expect("session exists");

	block_on(registry.sweep(NOW + DAY_MS)).expect("swept");

	assert_eq!(
		block_on(stream.collect::<Vec<_>>()),
		vec![
			HostFundingStatusSubscribeItem::InProgress { expires_at: Some(NOW + DAY_MS) },
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired },
		]
	);
}

#[test]
fn subscribing_drops_watchers_that_went_away() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	insert(&registry, session("fs_1", NOW));
	for _ in 0..3 {
		drop(registry.subscribe("fs_1").expect("session exists"));
	}
	let _live = registry.subscribe("fs_1").expect("session exists");

	assert_eq!(registry.lock_held().subscribers["fs_1"].len(), 1);
}

#[test]
fn a_restart_restores_open_sessions_and_expires_overdue_ones() {
	let db = open_memory_core_db();
	let before = registry_on(&db);
	insert(&before, session("fs_live", NOW));
	insert(&before, session("fs_overdue", NOW - DAY_MS));

	let after = registry_on(&db);
	block_on(after.sweep(NOW)).expect("loaded");

	assert_eq!(after.get("fs_live"), Some(session("fs_live", NOW)));
	assert_eq!(
		after
			.lock_held()
			.pack
			.get("fs_overdue")
			.map(|packed| packed.summary.stage.clone()),
		Some(FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms: NOW })
	);
}

// A provider that restarts after a withdrawal was released must still be
// handed it, or it could never report how the payout went; once it has,
// there is nothing left for it to do.
#[test]
fn a_released_withdrawal_is_served_until_its_payout_is_reported() {
	let registry = registry_on(&open_memory_core_db());
	let mut released =
		FundingSession { direction: FundingDirection::Out, ..session("fs_out", NOW) };
	assert!(released.assign(
		&ProductId::for_test("ramp.dot"),
		crate::host_logic::funding::test_choice(),
		NOW
	));
	released.updates.push(crate::host_logic::funding::FundingUpdateRecord {
		update: FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [4; 32],
			amount: 100,
		}),
		at_ms: NOW,
	});
	assert!(released.settle(100, NOW));
	insert(&registry, released);
	let first_served = || {
		registry
			.pending_calls(&ProductId::for_test("ramp.dot"))
			.into_iter()
			.find_map(|call| match call {
				ProviderCall::Assign(session) => Some(session.intent),
				ProviderCall::Cancel(..) => None,
			})
	};

	let before = first_served();
	let reported = block_on(registry.report(
		NOW + 1,
		&worker("ramp.dot"),
		"fs_out",
		FundingUpdate::Out(FundingOutboundUpdate::Payout {
			outcome: truapi::latest::FundingPayout::PaidOut,
		}),
	))
	.expect("stored");
	let after = first_served();

	assert_eq!((before, reported, after), (Some("fs_out".to_string()), Ok(()), None));
}

fn worker(provider_id: &str) -> ProductContext {
	ProductContext::new_with_execution(ProductId::for_test(provider_id), ExecutableKind::Worker)
}

/// Funds a report named, as the core follows them.
fn named_funds_record(host_id: [u8; 32], amount: u128) -> StartedFunds {
	StartedFunds {
		direction: FundingDirection::In,
		host_id,
		amount,
		terms: [0; 32],
		recorded_at_ms: NOW,
		accepted: true,
		named_at_ms: Some(NOW),
	}
}

/// Record funds `provider` starts under `host_id` for session `intent`.
fn record(
	registry: &FundingRegistry,
	provider: &str,
	intent: &str,
	host_id: [u8; 32],
	amount: u128,
) -> Option<StartRecord> {
	block_on(registry.record_start(
		NOW,
		&ProductId::for_test(provider),
		intent,
		FundsAsked { direction: FundingDirection::In, host_id, amount, terms: [0; 32] },
	))
	.expect("stored")
}

// Funds belong to the session the provider names when it starts them,
// whatever their amount: a first top-up of 100 for session B, which asks
// 200, is B's even while session A asks 100, so A can still be
// cancelled and B can name it.
#[test]
fn started_funds_belong_to_the_session_they_were_started_for() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	for (intent, amount) in [("fs_a", 100), ("fs_b", 200)] {
		let mut session = FundingSession::new(
			intent.to_string(),
			Some(ProductId::for_test("wallet.dot")),
			FundingDirection::In,
			Some(amount),
			NOW,
		);
		assert!(session.assign(&ramp(), crate::host_logic::funding::test_choice(), NOW));
		insert(&registry, session);
	}
	let host_id = host_payment_id(&worker("ramp.dot"), [1; 32]);
	let recorded = record(&registry, "ramp.dot", "fs_b", host_id, 100);
	block_on(registry.finish_start(NOW, "fs_b", host_id, true)).expect("stored");
	let report = |intent: &str| {
		block_on(registry.report(
			NOW,
			&worker("ramp.dot"),
			intent,
			FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 }),
		))
		.expect("stored")
	};

	assert_eq!(
		(
			recorded,
			report("fs_a"),
			report("fs_b"),
			block_on(registry.cancel(NOW, "fs_a")).expect("stored"),
			registry.get("fs_a").expect("kept").started,
		),
		(
			Some(StartRecord::New),
			Err(HostFundingReportError::NotStarted),
			Ok(()),
			CancelOutcome::Requested,
			Vec::new(),
		)
	);
}

// A withdrawal whose payout failed is refunded with a top-up, so it takes
// a top-up named for it, which moves in; a withdrawal still paying out
// takes none.
#[test]
fn a_refund_top_up_goes_to_the_withdrawal_whose_payout_failed() {
	let registry = registry_on(&open_memory_core_db());
	let withdrawal = |intent: &str, payout: Option<truapi::latest::FundingPayout>| {
		let mut withdrawal =
			FundingSession { direction: FundingDirection::Out, ..session(intent, NOW) };
		assert!(withdrawal.assign(
			&ProductId::for_test("ramp.dot"),
			crate::host_logic::funding::test_choice(),
			NOW
		));
		withdrawal.record_start(FundingDirection::Out, [4; 32], 80, [0; 32], NOW);
		withdrawal.accept_start(&[4; 32]);
		withdrawal
			.report(
				&ProductId::for_test("ramp.dot"),
				FundingUpdate::Out(FundingOutboundUpdate::Collecting {
					payment_id: [4; 32],
					amount: 80,
				}),
				|id| *id,
				NOW,
			)
			.expect("collecting");
		assert!(withdrawal.settle(80, NOW));
		if let Some(outcome) = payout {
			withdrawal
				.report(
					&ProductId::for_test("ramp.dot"),
					FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome }),
					|id| *id,
					NOW,
				)
				.expect("payout");
		}
		withdrawal
	};
	let failed = truapi::latest::FundingPayout::Failed { reason: "card closed".to_string() };
	insert(&registry, withdrawal("fs_failed", Some(failed)));
	insert(&registry, withdrawal("fs_paying", None));
	let refund = |intent: &str, host_id: [u8; 32]| {
		block_on(registry.record_start(
			NOW,
			&ProductId::for_test("ramp.dot"),
			intent,
			FundsAsked { direction: FundingDirection::In, host_id, amount: 80, terms: [0; 32] },
		))
		.expect("stored")
	};

	assert_eq!(
		(
			refund("fs_failed", [7; 32]),
			refund("fs_paying", [8; 32]),
			registry
				.get("fs_failed")
				.expect("kept")
				.pending_starts()
				.map(|funds| (funds.direction, funds.host_id))
				.collect::<Vec<_>>(),
		),
		(Some(StartRecord::New), None, vec![(FundingDirection::In, [7; 32])])
	);
}

// A top-up settles one session once: its id cannot be recorded for a
// second session of the provider, a session it was not started for
// cannot name it, and nor can another provider, or a session the
// provider does not serve.
#[test]
fn started_funds_are_named_once_by_the_session_they_were_started_for() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	let served = |intent: &str, provider: &str| {
		let mut session = session(intent, NOW);
		assert!(session.assign(
			&ProductId::for_test(provider),
			crate::host_logic::funding::test_choice(),
			NOW
		));
		session
	};
	for session in
		[served("fs_a", "ramp.dot"), served("fs_b", "ramp.dot"), served("fs_c", "other.dot")]
	{
		insert(&registry, session);
	}
	let host_id = host_payment_id(&worker("ramp.dot"), [9; 32]);
	let recorded = record(&registry, "ramp.dot", "fs_a", host_id, 100);
	let again = record(&registry, "ramp.dot", "fs_b", host_id, 100);
	let unserved = record(&registry, "ramp.dot", "fs_c", host_id, 100);
	block_on(registry.finish_start(NOW, "fs_a", host_id, true)).expect("stored");
	let crediting =
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [9; 32], amount: 100 });
	let report = |provider: &str, intent: &str| {
		block_on(registry.report(NOW, &worker(provider), intent, crediting.clone()))
			.expect("stored")
	};

	assert_eq!(
		(
			(recorded, again, unserved),
			report("ramp.dot", "fs_a"),
			report("ramp.dot", "fs_b"),
			report("other.dot", "fs_c"),
			registry.get("fs_a").expect("kept").started,
		),
		(
			(Some(StartRecord::New), Some(StartRecord::Conflict), None),
			Ok(()),
			Err(HostFundingReportError::NotStarted),
			Err(HostFundingReportError::NotStarted),
			vec![named_funds_record(host_id, 100)],
		)
	);
}

// A claim that is not final yet can still be reversed, and a top-up the
// host does not know may only be slow to appear: neither settles the
// session until the host decides or the grace period runs out.
#[test]
fn only_a_final_claim_or_a_long_unknown_top_up_settles() {
	let funds = named_funds_record([1; 32], 100);
	let read = |statuses: Vec<_>, now_ms| {
		block_on(top_up_moved(stream::iter(statuses).boxed(), &funds, now_ms))
	};
	let not_found = || vec![Err(HostPaymentTopUpStatusSubscribeError::TopUpNotFound)];

	assert_eq!(
		(
			read(vec![Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false })], NOW),
			read(
				vec![
					Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: false }),
					Ok(HostPaymentTopUpStatusSubscribeItem::Claimed { finalized: true })
				],
				NOW
			),
			read(
				vec![Ok(HostPaymentTopUpStatusSubscribeItem::ClaimedPartially {
					actual_claimed: 40
				})],
				NOW
			),
			read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS - 1),
			read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS),
			read(
				vec![Err(HostPaymentTopUpStatusSubscribeError::Unknown {
					reason: "offline".into()
				})],
				NOW
			),
		),
		(None, Some(100), Some(40), None, Some(0), None)
	);
}

#[test]
fn a_payment_settles_from_its_completion_or_a_long_absence() {
	let funds = named_funds_record([1; 32], 100);
	let read = |statuses: Vec<_>, now_ms| {
		block_on(payment_moved(stream::iter(statuses).boxed(), &funds, now_ms))
	};
	let not_found = || vec![Err(HostPaymentStatusSubscribeError::PaymentNotFound)];

	assert_eq!(
		(
			read(vec![Ok(HostPaymentStatusSubscribeItem::Processing)], NOW),
			read(vec![Ok(HostPaymentStatusSubscribeItem::Completed)], NOW),
			read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS - 1),
			read(not_found(), NOW + UNKNOWN_FUNDS_GRACE_MS),
		),
		(None, Some(100), None, Some(0))
	);
}

/// A host that, told of a change, cancels the next session in
/// `cancels` from inside the callback, as a host recording its history
/// there would acknowledge from there.
struct ReentrantHost {
	registry: OnceLock<Weak<FundingRegistry>>,
	cancels: Mutex<VecDeque<&'static str>>,
	/// Each session the host heard of, and whether the write lock was
	/// free at the time.
	heard: Mutex<Vec<(String, bool)>>,
}

#[async_trait::async_trait]
impl FundingPlatform for ReentrantHost {
	async fn present_funding(
		&self,
		_product: Option<&ProductContext>,
		_session: FundingPresentation,
	) -> Result<FundingPresentOutcome, GenericError> {
		Ok(FundingPresentOutcome::Dismissed)
	}

	async fn present_provider_frame(
		&self,
		_provider: &ProductContext,
		_intent: String,
		_target: truapi::latest::FundingFrameTarget,
	) -> Result<truapi::latest::FundingFrameOutcome, GenericError> {
		Ok(truapi::latest::FundingFrameOutcome::Closed)
	}

	fn close_provider_frame(&self, _provider: &ProductContext, _intent: String) {}

	fn provider_frame_requested(&self, _provider: &ProductContext, _intent: String) {}

	fn funding_session_changed(&self, intent: String, _status: HostFundingStatusSubscribeItem) {
		let registry = self.registry.get().and_then(Weak::upgrade).expect("registry alive");
		// Calling back while the lock is held would hang, so that is
		// recorded instead of attempted.
		let writes_free = registry.writes.try_lock().is_some();
		self.heard.lock().expect("heard mutex poisoned").push((intent, writes_free));
		let next = self.cancels.lock().expect("cancels mutex poisoned").pop_front();
		if let (true, Some(next)) = (writes_free, next) {
			std::thread::spawn(move || block_on(registry.cancel(NOW, next)))
				.join()
				.expect("cancel thread")
				.expect("cancelled");
		}
	}
}

// A host told of a change may act on it straight away, and must still
// hear of every change in the order it was made.
#[test]
fn the_host_hears_of_changes_in_order_and_may_call_back() {
	let db = open_memory_core_db();
	let registry = Arc::new(registry_on(&db));
	insert(&registry, session("fs_a", NOW));
	insert(&registry, session("fs_b", NOW));
	let host = Arc::new(ReentrantHost {
		registry: OnceLock::new(),
		cancels: Mutex::new(VecDeque::from(["fs_b"])),
		heard: Mutex::new(Vec::new()),
	});
	assert!(host.registry.set(Arc::downgrade(&registry)).is_ok());
	assert!(registry.install_platform(host.clone()));

	assert_eq!(block_on(registry.cancel(NOW, "fs_a")).expect("cancelled"), CancelOutcome::Ended);

	assert_eq!(
		*host.heard.lock().expect("heard mutex poisoned"),
		[("fs_a".to_string(), true), ("fs_b".to_string(), true)]
	);
}

#[test]
fn a_failed_save_changes_nothing_in_memory() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	insert(&registry, session("fs_1", NOW));
	block_on(fail_writes(&db, true));

	let failed = block_on(registry.commit(NOW, |draft| {
		draft.sessions.clear();
		((), Vec::new())
	}));

	assert!(failed.is_err());
	assert_eq!(registry.get("fs_1"), Some(session("fs_1", NOW)));
}

#[test]
fn a_session_whose_start_failed_is_not_stored_by_a_later_commit() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	block_on(registry.present(NOW, session("fs_1", NOW))).expect("presented");
	block_on(fail_writes(&db, true));

	let started = block_on(registry.start(NOW, "fs_1"));
	block_on(fail_writes(&db, false));
	insert(&registry, session("fs_2", NOW));

	assert!(started.is_err());
	assert_eq!(registry.get("fs_1"), None);
	assert_eq!(block_on(stored_sessions(&db)), vec![session("fs_2", NOW)]);
}

/// A funding surface that answers everything and hears nothing.
struct SilentPlatform;

#[async_trait::async_trait]
impl FundingPlatform for SilentPlatform {
	async fn present_funding(
		&self,
		_product: Option<&ProductContext>,
		_session: FundingPresentation,
	) -> Result<FundingPresentOutcome, GenericError> {
		Ok(FundingPresentOutcome::Dismissed)
	}

	async fn present_provider_frame(
		&self,
		_provider: &ProductContext,
		_intent: String,
		_target: truapi::latest::FundingFrameTarget,
	) -> Result<truapi::latest::FundingFrameOutcome, GenericError> {
		Ok(truapi::latest::FundingFrameOutcome::Closed)
	}

	fn close_provider_frame(&self, _provider: &ProductContext, _intent: String) {}

	fn provider_frame_requested(&self, _provider: &ProductContext, _intent: String) {}
}

/// A host feed that records each session it is told changed.
#[derive(Default)]
struct RecordingFeed {
	pushed: Mutex<Vec<String>>,
}

impl RecordingFeed {
	fn sessions(&self, intent: &str) -> usize {
		lock(&self.pushed).iter().filter(|pushed| *pushed == intent).count()
	}

	fn clear(&self) {
		lock(&self.pushed).clear();
	}
}

impl FundingFeed for RecordingFeed {
	fn funding_session_changed(&self, intent: String) {
		lock(&self.pushed).push(intent);
	}

	fn funding_in_progress_changed(&self, _in_progress: bool) {}

	fn funding_options_changed(&self, _intent: String) {}
}

// A session compacted as it ends is still announced, and the host reads
// its ending back from the summary.
#[test]
fn a_session_compacted_as_it_ends_is_announced_and_read_back() {
	let registry = registry_on(&open_memory_core_db());
	let feed = Arc::new(RecordingFeed::default());
	assert!(registry.install_platform(Arc::new(SilentPlatform)));
	registry.install_feed(feed.clone());
	insert(&registry, session("fs_1", NOW - DAY_MS + 1));
	feed.clear();

	block_on(registry.sweep(NOW + 1)).expect("expired");

	assert_eq!(
		(registry.get("fs_1"), feed.sessions("fs_1"), registry.summary("fs_1").is_some()),
		(None, 1, true)
	);
}

// A product that reloads after the host recorded its session must still
// learn how it ended, not read it as a session that never existed.
#[test]
fn a_recorded_session_still_answers_its_owner_after_a_restart() {
	let db = open_memory_core_db();
	let before = registry_on(&db);
	insert(&before, session("fs_1", NOW - DAY_MS));
	block_on(before.sweep(NOW)).expect("expired");
	assert!(block_on(before.acknowledge(NOW, "fs_1")).expect("recorded"));
	let expired = HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired };

	let after = registry_on(&db);
	block_on(after.sweep(NOW)).expect("loaded");
	let replay = after.subscribe("fs_1").expect("recorded session kept");

	assert_eq!(
		(
			block_on(replay.collect::<Vec<_>>()),
			after.get("fs_1").is_some_and(|session| session.needs_handoff()),
			after.subscribe("fs_9").is_none()
		),
		(vec![expired], false, true)
	);
}

// A compacted session keeps what its history row shows: the provider,
// the quote it was chosen on, how it ended, the payout and the provider's
// references, across a restart.
#[test]
fn a_compacted_withdrawal_keeps_its_history_details_after_a_restart() {
	let db = open_memory_core_db();
	let before = registry_on(&db);
	let mut sent = FundingSession {
		direction: FundingDirection::Out,
		acknowledged: true,
		..session("fs_sent", NOW - 1)
	};
	assert!(sent.assign(
		&ProductId::for_test("ramp.dot"),
		crate::host_logic::funding::test_choice(),
		NOW - 1
	));
	sent.choice = Some(crate::host_logic::funding::FundingChoice {
		quote: truapi::latest::FundingQuote {
			quote_id: "q".into(),
			send_amount: 100,
			receive_amount: 95,
			provider_fee: 5,
			network_fee: 0,
			eta_secs: None,
			expires_at: None,
			asset_decimals: 2,
		},
		rail: truapi::latest::FundingRail::Bank,
		asset: "EUR".into(),
		network: None,
	});
	for update in [
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [4; 32], amount: 100 }),
		FundingUpdate::Details {
			transaction_id: Some("tx-1".into()),
			reference: Some("ref-1".into()),
		},
		FundingUpdate::Out(FundingOutboundUpdate::Payout {
			outcome: truapi::latest::FundingPayout::PaidOut,
		}),
	] {
		sent.updates
			.push(crate::host_logic::funding::FundingUpdateRecord { update, at_ms: NOW - 1 });
	}
	assert!(sent.settle(100, NOW - 1));
	let newer = |n: u64| FundingSession {
		acknowledged: true,
		stage: FundingStage::Delivered { credited: 1, settled_at_ms: NOW + n },
		..session(&format!("fs_{n}"), NOW)
	};
	block_on(before.commit(NOW, |draft| {
		draft.sessions.insert("fs_sent".to_string(), sent);
		for n in 0..3 {
			draft.sessions.insert(format!("fs_{n}"), newer(n));
		}
		((), Vec::new())
	}))
	.expect("stored");

	let after = registry_on(&db);
	block_on(after.sweep(NOW)).expect("loaded");
	let row = after.history().into_iter().find(|row| row.intent == "fs_sent");

	assert_eq!(
		(
			after.get("fs_sent").is_none(),
			row.map(|row| (
				row.provider_id,
				row.choice.map(|choice| (choice.rail, choice.asset, choice.quote.send_amount)),
				row.status,
				row.payout,
				row.transaction_id,
				row.reference
			))
		),
		(
			true,
			Some((
				Some(ProductId::for_test("ramp.dot")),
				Some((truapi::latest::FundingRail::Bank, "EUR".to_string(), 100)),
				crate::host_logic::funding::FundingStatus::Sent,
				Some(truapi::latest::FundingPayout::PaidOut),
				Some("tx-1".to_string()),
				Some("ref-1".to_string())
			))
		)
	);
}

// An ended session is compacted, not dropped: after a restart its owner
// still reads how it ended, and history still lists every ended session.
#[test]
fn a_compacted_session_still_answers_its_owner_after_a_restart() {
	let db = open_memory_core_db();
	let before = registry_on(&db);
	let recorded = |n: u64| FundingSession {
		acknowledged: true,
		stage: FundingStage::Delivered { credited: u128::from(n) + 1, settled_at_ms: NOW + n },
		..session(&format!("fs_{n}"), NOW)
	};
	block_on(before.commit(NOW, |draft| {
		for n in 0..=2 {
			draft.sessions.insert(format!("fs_{n}"), recorded(n));
		}
		((), Vec::new())
	}))
	.expect("stored");

	let after = registry_on(&db);
	block_on(after.sweep(NOW)).expect("loaded");
	let replay = after.subscribe("fs_0").expect("compacted session kept");

	assert_eq!(
		(
			after.get("fs_0"),
			block_on(replay.collect::<Vec<_>>()),
			after.owner("fs_0"),
			after.get("fs_1").is_some(),
			after.history(),
		),
		(
			None,
			vec![HostFundingStatusSubscribeItem::Delivered { credited: 1 }],
			Some(Some(ProductId::for_test("wallet.dot"))),
			false,
			(0..=2).rev().map(|n| recorded(n).summary()).collect::<Vec<_>>(),
		)
	);
}

// Showing a session in the overlay stores nothing, so it cannot fail on a
// store that is down and leaves the stored set as it was.
#[test]
fn holding_a_session_in_the_overlay_writes_nothing() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	insert(&registry, session("fs_1", NOW));
	block_on(fail_writes(&db, true));

	let presented = block_on(registry.present(NOW, session("fs_2", NOW)));

	assert_eq!((presented, block_on(stored_sessions(&db))), (Ok(true), vec![session("fs_1", NOW)]));
}

// An intent names one session for good, so one held in full, presented or
// compacted cannot be presented again.
#[test]
fn a_held_intent_cannot_be_presented_again() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	insert(&registry, session("fs_1", NOW));
	registry.lock_held().pack.insert(
		"fs_packed".to_string(),
		PackedSession::from(&FundingSession {
			stage: FundingStage::Delivered { credited: 1, settled_at_ms: NOW },
			..session("fs_packed", NOW)
		}),
	);

	assert_eq!(
		[
			block_on(registry.present(NOW, session("fs_1", NOW))),
			block_on(registry.present(NOW, session("fs_packed", NOW))),
			block_on(registry.present(NOW, session("fs_new", NOW))),
			block_on(registry.present(NOW, session("fs_new", NOW))),
		],
		[Ok(false), Ok(false), Ok(true), Ok(false)]
	);
}

// A product watching a session the user then dismissed hears that it
// ended, rather than waiting on a session that is gone.
#[test]
fn a_discarded_session_ends_its_watchers_as_cancelled() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	assert_eq!(block_on(registry.present(NOW, session("fs_1", NOW))), Ok(true));
	let watch = registry.subscribe("fs_1").expect("presented session watchable");

	block_on(registry.discard("fs_1"));

	assert_eq!(
		(block_on(watch.collect::<Vec<_>>()), registry.get("fs_1")),
		(
			vec![
				HostFundingStatusSubscribeItem::InProgress { expires_at: Some(NOW + DAY_MS) },
				HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
			],
			None,
		)
	);
}

// Sessions a newer app stored are left alone: every commit fails, and none
// writes over them.
#[test]
fn sessions_of_a_newer_format_make_the_registry_read_only() {
	let db = open_memory_core_db();
	block_on(put_session_row(&db, "fs_1", (u16::MAX, session("fs_1", NOW)).encode()));
	let before = block_on(rows(&db, "funding_session"));
	let registry = registry_on(&db);

	let presented = block_on(registry.present(NOW, session("fs_2", NOW)));

	assert_eq!(
		(presented, registry.get("fs_2"), block_on(rows(&db, "funding_session"))),
		(Err(FundingSessionError::NewerFormat { format: u16::MAX }), None, before)
	);
}

#[test]
fn a_session_whose_overlay_is_up_cannot_be_cancelled() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	block_on(registry.present(NOW, session("fs_1", NOW))).expect("presented");

	assert_eq!(block_on(registry.cancel(NOW, "fs_1")), Ok(CancelOutcome::Refused));
	assert_eq!(registry.get("fs_1").map(|session| session.is_terminal()), Some(false));
}

// A selection is the overlay's own: closing the overlay drops it, and one
// the host reports as the overlay closes is not kept for a session that
// no longer has one.
#[test]
fn a_selection_goes_with_its_overlay() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	let picked = FundingSelection {
		rail: truapi::latest::FundingRail::Card,
		asset: Some("EUR".to_string()),
		network: None,
		country: None,
		currency: None,
	};
	block_on(registry.present(NOW, session("fs_1", NOW))).expect("presented");
	let while_up = (registry.select("fs_1", picked.clone()), registry.selection("fs_1"));
	block_on(registry.start(NOW, "fs_1")).expect("started");

	assert_eq!(
		(while_up, registry.select("fs_1", picked.clone()), registry.selection("fs_1")),
		((true, Some(picked)), false, None)
	);
}

// A row that does not decode is the core's only trace of that session, so
// no later commit writes over it or deletes it.
#[test]
fn a_row_that_does_not_decode_survives_later_commits() {
	let db = open_memory_core_db();
	block_on(put_session_row(&db, "fs_bad", vec![0xff]));
	let registry = registry_on(&db);

	insert(&registry, session("fs_1", NOW));
	block_on(registry.sweep(NOW + DAY_MS)).expect("expired");

	let bad = block_on(rows(&db, "funding_session"))
		.into_iter()
		.find(|(intent, _)| intent == "fs_bad");
	assert_eq!((registry.get("fs_bad"), bad), (None, Some(("fs_bad".to_string(), vec![0xff]))));
}

// Compaction moves sessions out of the full record and into the pack in
// one write, so a failed write leaves both as they were.
#[test]
fn a_compaction_that_fails_to_store_keeps_every_session() {
	let db = open_memory_core_db();
	let registry = registry_on(&db);
	insert(&registry, session("fs_0", NOW));
	insert(&registry, session("fs_1", NOW));
	block_on(fail_writes(&db, true));

	let compacted = block_on(registry.commit(NOW, |draft| {
		for session in draft.sessions.values_mut() {
			session.stage = FundingStage::Delivered { credited: 1, settled_at_ms: NOW };
		}
		((), Vec::new())
	}));

	assert_eq!(
		(
			compacted.is_err(),
			registry.get("fs_0").is_some_and(|session| !session.is_terminal()),
			registry.history().len(),
			block_on(rows(&db, "funding_session")).len(),
			block_on(rows(&db, "funding_summary")).len(),
		),
		(true, true, 0, 2, 0)
	);
}

// A frame that replaced another for the same session stays tracked when the
// one it replaced goes, so it is still closed once the payment moves.
#[test]
fn a_replaced_frame_leaves_its_successor_tracked() {
	let registry = FundingRegistry::default();
	let first = registry.track_frame("fs_1", worker("ramp.dot"));
	let second = registry.track_frame("fs_1", worker("ramp.dot"));
	drop(first);
	let tracked = lock(&registry.frames).contains_key("fs_1");
	drop(second);
	assert_eq!((tracked, lock(&registry.frames).contains_key("fs_1")), (true, false));
}

// A cancel that moved no money is left out of history, yet its summary still
// answers for the session's own ending screen.
#[test]
fn an_unmoved_cancel_leaves_history_but_keeps_its_summary() {
	let registry = registry_on(&open_memory_core_db());
	insert(
		&registry,
		FundingSession {
			stage: FundingStage::Failed { reason: FundingFailure::Cancelled, settled_at_ms: NOW },
			..session("fs_1", NOW)
		},
	);
	block_on(registry.sweep(NOW + 1)).expect("swept");
	assert_eq!((registry.history().len(), registry.summary("fs_1").is_some()), (0, true));
}
