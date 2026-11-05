use super::*;

const NOW: u64 = 1_700_000_000_000;
fn provider() -> ProductId {
	ProductId::for_test("ramp.dot")
}

// The host opens a provider page in its own frame, so only an https page
// it can show the origin of reaches it, never one carrying credentials
// or another scheme, and the return URL is held to the same rule.
#[test]
fn a_frame_opens_only_plain_https_pages() {
	let page = |url: &str, return_url: Option<&str>| {
		checked_frame_target(FundingFrameTarget::Page {
			url: url.to_string(),
			return_url: return_url.map(str::to_string),
		})
	};
	let widget = "https://widget.example.com/buy?amount=10&token=a%2Fb";

	assert_eq!(
		(
			page(widget, Some("https://onramp.example.com/meld/return?flow=sell")),
			page("http://widget.example.com/", None),
			page("https://user:pass@widget.example.com/", None),
			page("javascript:alert(1)", None),
			page(widget, Some("polkadotapp://done")),
			checked_frame_target(FundingFrameTarget::App { route: "/kyc".to_string() }),
		),
		(
			Some(FundingFrameTarget::Page {
				url: widget.to_string(),
				return_url: Some("https://onramp.example.com/meld/return?flow=sell".to_string()),
			}),
			None,
			None,
			None,
			None,
			Some(FundingFrameTarget::App { route: "/kyc".to_string() }),
		)
	);
}

#[test]
fn an_app_route_stays_inside_the_provider_app() {
	let route = |route: &str| match checked_frame_target(FundingFrameTarget::App {
		route: route.to_string(),
	}) {
		Some(FundingFrameTarget::App { route }) => Some(route),
		_ => None,
	};

	assert_eq!(
		[
			route("/kyc"),
			route("deposit?step=2#top"),
			route("/a/../kyc"),
			route("//evil.example/x"),
			route("https://evil.example/"),
			route("javascript:alert(1)"),
			route("\\\\evil.example"),
			route("/\\evil.example"),
			// The base the core resolves against is no way in either.
			route("https://app.invalid/x"),
			route("//app.invalid/x"),
			route("https://user:secret@app.invalid/"),
			route("https://app.invalid:8443/"),
		],
		[
			Some("/kyc".to_string()),
			Some("/deposit?step=2#top".to_string()),
			Some("/kyc".to_string()),
			None,
			None,
			None,
			None,
			None,
			None,
			None,
			None,
			None,
		]
	);
}

fn session(direction: FundingDirection) -> FundingSession {
	FundingSession::new(
		"fs_1".to_string(),
		Some(ProductId::for_test("wallet.dot")),
		direction,
		Some(100),
		NOW,
	)
}

fn expired(intent: &str, settled_at_ms: u64) -> FundingSession {
	FundingSession {
		intent: intent.to_string(),
		stage: FundingStage::Failed { reason: FundingFailure::Expired, settled_at_ms },
		..session(FundingDirection::In)
	}
}

// Each owner keeps its newest recorded sessions in full, so no owner can
// crowd out another, and its older ones are compacted rather than dropped.
#[test]
fn recorded_sessions_keep_the_newest_per_owner_and_compact_the_rest() {
	let recorded = |intent: String, owner: &str, settled_at_ms: u64| FundingSession {
		acknowledged: true,
		owner_product_id: Some(ProductId::for_test(owner)),
		..expired(&intent, settled_at_ms)
	};
	let mut sessions: Vec<FundingSession> = (0..RECORDED_PER_OWNER as u64 + 2)
		.map(|n| recorded(format!("a_{n}"), "a.dot", NOW + n))
		.collect();
	sessions.push(recorded("b_0".to_string(), "b.dot", NOW));
	sessions.push(session(FundingDirection::In));

	let (kept, compacted) = retained(sessions);
	let count = |owner: &str| {
		kept.iter()
			.filter(|session| session.owner_product_id.as_ref().is_some_and(|id| *id == owner))
			.count()
	};

	assert_eq!(
		(
			count("a.dot"),
			count("b.dot"),
			kept.iter().any(|session| session.intent == "a_0" || session.intent == "a_1"),
			kept.iter().any(|session| !session.is_terminal()),
			compacted,
		),
		(
			RECORDED_PER_OWNER,
			1,
			false,
			true,
			vec![
				FundingSessionSummary::from(recorded("a_1".to_string(), "a.dot", NOW + 1)),
				FundingSessionSummary::from(recorded("a_0".to_string(), "a.dot", NOW)),
			]
		)
	);
}

// A product only waits for the outcome; the host draws the steps, so an
// open session reads the same whichever way value moves.
#[test]
fn an_open_session_reads_in_progress_until_its_deadline() {
	let in_progress =
		HostFundingStatusSubscribeItem::InProgress { expires_at: Some(NOW + SESSION_WINDOW_MS) };
	assert_eq!(
		(session(FundingDirection::In).wire_item(), session(FundingDirection::Out).wire_item(),),
		(in_progress.clone(), in_progress)
	);
}

// Nothing else ends a session that hears nothing, so expiry is what
// guarantees every subscriber eventually gets a terminal item.
#[test]
fn a_session_expires_at_its_deadline_and_not_before() {
	let mut session = session(FundingDirection::In);

	assert!(!session.expire_if_due(NOW + SESSION_WINDOW_MS - 1));
	assert!(session.expire_if_due(NOW + SESSION_WINDOW_MS));
	assert_eq!(
		session.wire_item(),
		HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired }
	);
}

// A bank transfer can take days, so a provider that keeps reporting holds
// the session past the first day; one that went silent for a week loses
// it, so it cannot hold the session and its worker for good.
#[test]
fn an_assigned_session_expires_a_week_after_its_provider_last_reported() {
	let mut session = served(FundingDirection::In);
	let heard_at = NOW + SESSION_WINDOW_MS;
	session
		.report(
			&provider(),
			FundingUpdate::In(FundingInboundUpdate::AwaitingPayment),
			|id| *id,
			heard_at,
		)
		.expect("reported");

	let before = session.expire_if_due(heard_at + PROVIDER_SILENCE_MS - 1);
	let shown = session.wire_item();
	let at = session.expire_if_due(heard_at + PROVIDER_SILENCE_MS);

	assert_eq!(
		(before, shown, at, session.wire_item()),
		(
			false,
			HostFundingStatusSubscribeItem::InProgress {
				expires_at: Some(heard_at + PROVIDER_SILENCE_MS)
			},
			true,
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Expired },
		)
	);
}

// Once the provider holds the user's payment, expiring the session would
// write off money still owed to the user, so only its outcome ends it.
#[test]
fn a_session_whose_provider_holds_the_payment_never_expires() {
	let mut session = served(FundingDirection::In);
	session
		.report(
			&provider(),
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
				finalized: true,
				mismatch: None,
			}),
			|id| *id,
			NOW,
		)
		.expect("reported");

	let expired = session.expire_if_due(NOW + 10 * PROVIDER_SILENCE_MS);

	assert_eq!(
		(expired, session.wire_item()),
		(false, HostFundingStatusSubscribeItem::InProgress { expires_at: None })
	);
}

#[test]
fn an_ended_session_keeps_its_first_outcome() {
	let mut session = expired("fs_1", NOW);

	assert!(!session.fail(FundingFailure::Cancelled, NOW + 1));
	assert_eq!(session, expired("fs_1", NOW));
}

// The host owns the history: open sessions come first, then the ones it
// has not recorded, newest first and not without bound, then the recorded
// ones kept only so their owner can read how they ended. An unrecorded
// session past the bound is compacted, not lost.
#[test]
fn the_core_keeps_open_then_unrecorded_then_recorded_sessions() {
	let ended = |index: usize, acknowledged| FundingSession {
		acknowledged,
		..expired(&format!("fs_s{index}"), NOW + index as u64)
	};
	let open = session(FundingDirection::Out);
	let recorded = (0..3).map(|index| ended(index, true));
	let unrecorded = (100..101 + UNACKNOWLEDGED_LIMIT).map(|index| ended(index, false));

	let (kept, compacted) = retained(recorded.chain(unrecorded).chain([open]));
	let kept: Vec<String> = kept.into_iter().map(|session| session.intent).collect();

	let newest_unrecorded =
		(101..101 + UNACKNOWLEDGED_LIMIT).rev().map(|index| format!("fs_s{index}"));
	let recorded_newest_first = (0..3).rev().map(|index| format!("fs_s{index}"));
	assert_eq!(
		(kept, compacted),
		(
			std::iter::once("fs_1".to_string())
				.chain(newest_unrecorded)
				.chain(recorded_newest_first)
				.collect::<Vec<_>>(),
			vec![FundingSessionSummary::from(ended(100, false))],
		)
	);
}

fn served(direction: FundingDirection) -> FundingSession {
	let mut session = session(direction);
	assert!(session.assign(&provider(), test_choice(), NOW));
	session
}

/// Report `update` from the provider. Ids are compared as given, as the
/// session only checks them against what was recorded.
fn report(
	session: &mut FundingSession,
	update: FundingUpdate,
) -> Result<(), HostFundingReportError> {
	session.report(&provider(), update, |id| *id, NOW)
}

fn reported(session: &mut FundingSession, updates: &[FundingUpdate]) {
	for update in updates {
		if let Some((id, amount)) = update.named_funds() {
			taken(session, *id, amount);
		}
		report(session, update.clone())
			.unwrap_or_else(|refusal| panic!("{update:?} refused: {refusal:?}"));
	}
}

/// Record funds the provider starts, before the host takes them.
fn start(session: &mut FundingSession, host_id: [u8; 32], amount: u128) -> StartRecord {
	session.record_start(host_id, amount, [0; 32], NOW)
}

/// Record funds the provider started and the host took.
fn taken(session: &mut FundingSession, host_id: [u8; 32], amount: u128) {
	start(session, host_id, amount);
	session.accept_start(&host_id);
}

fn named(host_id: [u8; 32], amount: u128) -> StartedFunds {
	StartedFunds {
		host_id,
		amount,
		terms: [0; 32],
		recorded_at_ms: NOW,
		accepted: true,
		named_at_ms: Some(NOW),
	}
}

// With no provider to ask, a cancel ends the session itself; once it
// ended, its outcome stands.
#[test]
fn an_unserved_session_is_cancelled_once() {
	let mut open = session(FundingDirection::In);
	let mut ended = expired("fs_2", NOW);

	assert_eq!(
		(open.cancel(NOW + 1), open.wire_item(), ended.cancel(NOW + 1)),
		(
			CancelOutcome::Ended,
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
			CancelOutcome::Refused
		)
	);
}

fn cancelled(session: &mut FundingSession) -> CancelOutcome {
	session.cancel(NOW)
}

fn offer(fee: u128, refund: u128, expires_at: Option<u64>) -> FundingCancelAnswer {
	FundingCancelAnswer::Offer { fee, refund, expires_at }
}

fn refused(reason: &str) -> FundingCancelAnswer {
	FundingCancelAnswer::Refused { reason: reason.into() }
}

fn declined() -> Option<CancelCall> {
	Some(CancelCall::Settle(FundingCancelDecision::Declined))
}

// Once the user's payment reached the provider, a cancel is a request:
// asking again while it is pending changes nothing and never ends the
// session, and a refusal leaves it running until the user asks anew.
#[test]
fn after_money_moved_a_cancel_is_a_request_the_provider_answers() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: false,
			mismatch: None,
		})],
	);

	let asked = cancelled(&mut session);
	let again = cancelled(&mut session);
	session.answer_cancel(refused("card settled"), NOW);
	let refused = session.cancel.clone();
	let asked_anew = cancelled(&mut session);

	assert_eq!(
		(asked, again, refused, asked_anew, session.is_terminal()),
		(
			CancelOutcome::Requested,
			CancelOutcome::Pending,
			Some(FundingCancel::Refused { reason: "card settled".into() }),
			CancelOutcome::Requested,
			false,
		)
	);
}

// The user sees the provider's terms and declines them: the session runs
// on, and the provider hears the answer, again on each connect until it
// confirms it heard it.
#[test]
fn a_declined_offer_lets_the_session_carry_on() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut session);
	session.answer_cancel(offer(5, 95, None), NOW);
	let offered = session.cancel.clone();

	let answer = session.confirm_cancel(None, NOW);
	let pending = session.pending_cancel_call();
	let heard = session.cancel_settled(&FundingCancelDecision::Declined);

	assert_eq!(
		(offered, answer, session.is_terminal(), pending, heard, session.pending_cancel_call()),
		(
			Some(FundingCancel::Offered { fee: 5, refund: 95, expires_at: None }),
			CancelConfirmation::Declined,
			false,
			declined(),
			true,
			None,
		)
	);
}

// A provider that reports its next step after a decline has carried on,
// so the decline is not handed to it again on every connect.
#[test]
fn a_decline_stops_repeating_once_the_provider_carries_on() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut session);
	session.answer_cancel(offer(5, 95, None), NOW);
	session.confirm_cancel(None, NOW);
	let before = session.pending_cancel_call();

	reported(&mut session, &[FundingUpdate::In(FundingInboundUpdate::Converting)]);

	assert_eq!(
		(before, session.pending_cancel_call(), session.cancel.clone()),
		(declined(), None, None)
	);
}

// An accepted offer is the provider's to settle: before any credit it
// returns the user's payment and fails the session, which it may do
// although it holds the payment.
#[test]
fn an_accepted_offer_is_settled_by_the_provider() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut session);
	session.answer_cancel(offer(5, 95, None), NOW);

	let answer = session.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW);
	let heard = session.cancel_settled(&FundingCancelDecision::Accepted { fee: 5, refund: 95 });
	let replayed = session.pending_cancel_call();
	let settled = report(&mut session, FundingUpdate::Failed { reason: FundingFailure::Cancelled });

	assert_eq!(
		(answer, heard, replayed, settled, session.wire_item(), session.cancel),
		(
			CancelConfirmation::Accepted,
			false,
			Some(CancelCall::Settle(FundingCancelDecision::Accepted { fee: 5, refund: 95 })),
			Ok(()),
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
			None,
		)
	);
}

// An offer the user leaves unanswered lapses on the core's clock, which
// keeps time for it although nothing else can expire the session; an
// answer after that cannot accept it.
#[test]
fn an_offer_lapses_at_its_expiry() {
	let mut lapsing = served(FundingDirection::In);
	reported(
		&mut lapsing,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut lapsing);
	lapsing.answer_cancel(offer(5, 95, Some(NOW + 10)), NOW);
	let mut late = lapsing.clone();

	let alarm = lapsing.next_alarm_ms();
	let early = lapsing.lapse_cancel_offer(NOW + 9);
	let due = lapsing.lapse_cancel_offer(NOW + 10);

	assert_eq!(
		(
			alarm,
			early,
			due,
			lapsing.cancel.clone(),
			lapsing.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 11),
			late.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 10),
			late.cancel,
		),
		(
			Some(NOW + 10),
			false,
			true,
			Some(FundingCancel::Declined),
			CancelConfirmation::NoOffer,
			CancelConfirmation::Lapsed,
			Some(FundingCancel::Declined),
		)
	);
}

// Once the provider started crediting the user's balance, there is
// nothing left to cancel: the host tells the user it is too late, and
// the provider is not asked.
#[test]
fn an_inbound_session_whose_credit_started_is_too_late_to_cancel() {
	let mut recorded = served(FundingDirection::In);
	start(&mut recorded, [1; 32], 100);
	let mut named = served(FundingDirection::In);
	reported(
		&mut named,
		&[FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 })],
	);

	assert_eq!(
		(cancelled(&mut recorded), recorded.cancel, cancelled(&mut named)),
		(CancelOutcome::TooLate, None, CancelOutcome::TooLate)
	);
}

// A worker can stop between starting a payment and naming it. The
// payment it recorded locks the session as named funds do: asking twice
// does not end it, no deadline does, and the provider cannot fail it, so
// a debited user is never told nothing moved.
#[test]
fn recorded_funds_lock_the_session_before_they_are_named() {
	let mut session = served(FundingDirection::Out);
	start(&mut session, [3; 32], 40);

	let first = cancelled(&mut session);
	let second = cancelled(&mut session);
	let expired = session.expire_if_due(u64::MAX);
	let failed = report(&mut session, FundingUpdate::Failed { reason: FundingFailure::Cancelled });

	assert_eq!(
		(first, second, expired, session.expires_at(), failed, session.is_terminal()),
		(
			CancelOutcome::Requested,
			CancelOutcome::Pending,
			false,
			None,
			Err(HostFundingReportError::OutOfOrder),
			false,
		)
	);
}

// The provider makes one offer per cancel: a second answer to the same
// cancel is not taken, and terms other than the offer's accept nothing.
#[test]
fn an_offer_stands_until_the_user_answers_it() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut session);
	let first = session.answer_cancel(offer(5, 95, None), NOW);
	let second = session.answer_cancel(offer(20, 80, None), NOW);

	let other = session.confirm_cancel(Some(CancelTerms { fee: 20, refund: 80 }), NOW);
	let asked = session.pending_cancel_call();
	let shown = session.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW);

	assert_eq!(
		(first, second, other, asked, shown, session.cancel),
		(
			true,
			false,
			CancelConfirmation::NoOffer,
			None,
			CancelConfirmation::Accepted,
			Some(FundingCancel::Accepted { fee: 5, refund: 95 }),
		)
	);
}

// Before money moved the provider stops, and the core ends the session as
// cancelled; once money moved, stopping is not the provider's answer to
// give, so it counts as a refusal and the user can ask again.
#[test]
fn a_provider_stops_only_before_money_moved() {
	let mut fresh = served(FundingDirection::In);
	cancelled(&mut fresh);
	let mut paid = served(FundingDirection::In);
	reported(
		&mut paid,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut paid);

	assert_eq!(
		(
			fresh.answer_cancel(FundingCancelAnswer::Stopped, NOW),
			fresh.wire_item(),
			paid.answer_cancel(FundingCancelAnswer::Stopped, NOW),
			paid.cancel.clone(),
			cancelled(&mut paid),
			paid.is_terminal(),
		),
		(
			true,
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
			true,
			Some(FundingCancel::Refused { reason: String::new() }),
			CancelOutcome::Requested,
			false,
		)
	);
}

// An inbound offer accepted after the provider started crediting comes
// too late: the provider could not honour it, so it is declined.
#[test]
fn accepting_an_inbound_offer_after_the_credit_started_is_too_late() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut session);
	session.answer_cancel(offer(5, 95, None), NOW);
	start(&mut session, [1; 32], 100);

	let answer = session.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW);

	assert_eq!(
		(answer, session.cancel.clone(), session.pending_cancel_call()),
		(CancelConfirmation::TooLate, Some(FundingCancel::Declined), declined())
	);
}

// A detail reported after the provider handed over the outcome must not
// stop the session from settling.
#[test]
fn a_detail_after_delivered_still_settles() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	start(&mut session, [1; 32], 100);
	reported(
		&mut session,
		&[
			FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 }),
			FundingUpdate::In(FundingInboundUpdate::Delivered),
			FundingUpdate::Details { transaction_id: Some("tx".into()), reference: None },
		],
	);

	assert!(
		matches!(session.settlement_due(), Some(Settlement::TopUps(top_ups)) if top_ups.len() == 1)
	);
}

// An offer cannot hand back more than the session moves, an inbound
// session whose credit started cannot be offered, and an outbound offer
// waits for the refund path of a released withdrawal. One that does not
// fit counts as a refusal, so the cancel does not wait on it for good.
#[test]
fn an_offer_must_fit_the_session() {
	let mut over = served(FundingDirection::In);
	reported(
		&mut over,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut over);
	let mut crediting = over.clone();
	start(&mut crediting, [1; 32], 100);
	let mut outbound = served(FundingDirection::Out);
	cancelled(&mut outbound);

	assert_eq!(
		(
			over.answer_cancel(offer(5, 100, None), NOW),
			crediting.answer_cancel(offer(5, 95, None), NOW),
			outbound.answer_cancel(offer(5, 95, None), NOW),
			over.cancel,
		),
		(true, true, true, Some(FundingCancel::Refused { reason: String::new() }))
	);
}

// A refusal after the payment was named leaves the payment to settle
// the session, before and after a restart.
#[test]
fn a_cancel_answer_after_collecting_does_not_hide_the_settlement() {
	let mut session = served(FundingDirection::Out);
	reported(
		&mut session,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 40,
		})],
	);
	cancelled(&mut session);
	session.answer_cancel(refused("paid out"), NOW);
	let restarted =
		FundingSession::decode(&mut &session.encode()[..]).expect("stored session decodes");

	let due = |session: &FundingSession| match session.settlement_due() {
		Some(Settlement::Payment(funds)) => Some(funds.amount),
		_ => None,
	};
	assert_eq!((due(&session), due(&restarted)), (Some(40), Some(40)));
}

// A provider answers only a cancel the user asked for, and an accepted
// offer is not taken back: it is the provider's to settle.
#[test]
fn a_cancel_answer_needs_a_pending_cancel() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	let unasked = session.answer_cancel(offer(5, 95, None), NOW);
	cancelled(&mut session);
	session.answer_cancel(offer(2, 98, None), NOW);
	session.confirm_cancel(Some(CancelTerms { fee: 2, refund: 98 }), NOW);

	assert_eq!(
		(unasked, session.answer_cancel(refused("late"), NOW), session.cancel),
		(false, false, Some(FundingCancel::Accepted { fee: 2, refund: 98 }))
	);
}

// An answer to a cancel is not an update: a restarted worker resumes from
// its last step, and the next step still follows.
#[test]
fn cancel_answers_sit_outside_the_order_of_steps() {
	let mut session = served(FundingDirection::In);
	let received = FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
		finalized: true,
		mismatch: None,
	});
	reported(&mut session, std::slice::from_ref(&received));
	cancelled(&mut session);
	session.answer_cancel(refused("no"), NOW);

	assert_eq!(
		(
			session.assignment().last_update,
			report(&mut session, FundingUpdate::In(FundingInboundUpdate::Converting))
		),
		(Some(received), Ok(()))
	);
}

// A provider that never answers the first cancel cannot hold the session:
// asking again ends it.
#[test]
fn a_second_cancel_ends_a_session_its_provider_left_unanswered() {
	let mut session = served(FundingDirection::Out);

	assert_eq!(
		(session.cancel(NOW), session.cancel(NOW + 1), session.wire_item()),
		(
			CancelOutcome::Requested,
			CancelOutcome::Ended,
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::Cancelled },
		)
	);
}

#[test]
fn a_session_is_assigned_once() {
	let mut session = served(FundingDirection::In);

	assert!(!session.assign(&ProductId::for_test("other.dot"), test_choice(), NOW));
	assert_eq!(session.provider_id, Some(provider()));
}

// Reports come only from the assigned provider and only move forward, so
// the stored timeline is one the host can draw in order.
#[test]
fn reports_come_from_the_provider_and_only_move_forward() {
	let mut session = served(FundingDirection::In);
	reported(&mut session, &[FundingUpdate::In(FundingInboundUpdate::Converting)]);

	assert_eq!(
		(
			session.report(
				&ProductId::for_test("other.dot"),
				FundingUpdate::In(FundingInboundUpdate::Converting),
				|id| *id,
				NOW
			),
			report(&mut session, FundingUpdate::In(FundingInboundUpdate::AwaitingPayment)),
			report(&mut session, FundingUpdate::In(FundingInboundUpdate::Converting)),
			report(
				&mut session,
				FundingUpdate::Out(FundingOutboundUpdate::Collecting {
					payment_id: [1; 32],
					amount: 5
				})
			),
			report(&mut session, FundingUpdate::In(FundingInboundUpdate::Delivered)),
		),
		(
			Err(HostFundingReportError::NotFound),
			Err(HostFundingReportError::OutOfOrder),
			Err(HostFundingReportError::OutOfOrder),
			Err(HostFundingReportError::OutOfOrder),
			Err(HostFundingReportError::OutOfOrder),
		)
	);
}

// The core settles from what it saw the provider start and the host
// take, so a provider cannot claim a top-up it never started for this
// session, one the host has not taken, an inflated one, or one twice.
#[test]
fn a_report_names_only_funds_the_host_took_for_the_session_at_their_amount() {
	let mut session = served(FundingDirection::In);
	start(&mut session, [1; 32], 70);

	let never_started = report(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [9; 32], amount: 70 }),
	);
	let not_taken = report(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 70 }),
	);
	session.accept_start(&[1; 32]);
	let inflated = report(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 700 }),
	);
	let credited = report(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 70 }),
	);
	let twice = report(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 70 }),
	);

	assert_eq!(
		(never_started, not_taken, inflated, credited, twice, session.started),
		(
			Err(HostFundingReportError::NotStarted),
			Err(HostFundingReportError::NotStarted),
			Err(HostFundingReportError::NotStarted),
			Ok(()),
			Err(HostFundingReportError::OutOfOrder),
			vec![named([1; 32], 70)],
		)
	);
}

fn chosen(direction: FundingDirection, rail: FundingRail) -> FundingSession {
	let mut session = session(direction);
	let quote = FundingQuote {
		quote_id: "q1".to_string(),
		send_amount: 100,
		receive_amount: 99,
		provider_fee: 1,
		network_fee: 0,
		eta_secs: None,
		expires_at: None,
	};
	let choice = FundingChoice { quote, rail, asset: "EUR".to_string() };
	assert!(session.assign(&provider(), choice, NOW));
	session
}

fn report_at(session: &mut FundingSession, update: FundingUpdate, at_ms: u64) {
	if let Some((id, amount)) = update.named_funds() {
		taken(session, *id, amount);
	}
	session.report(&provider(), update, |id| *id, at_ms).expect("reported");
}

fn steps(progress: &FundingProgress) -> Vec<(FundingStep, Option<u64>)> {
	progress.steps.iter().map(|step| (step.step, step.reached_at_ms)).collect()
}

// Hosts decide from this whether a second cancel can still end the
// session, so it must say exactly what the core's own rule says.
#[test]
fn progress_reports_money_moved_as_the_core_rules_it() {
	let fresh = served(FundingDirection::In);
	let mut paid = served(FundingDirection::In);
	reported(
		&mut paid,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: false,
			mismatch: None,
		})],
	);
	let mut started = served(FundingDirection::Out);
	assert_eq!(start(&mut started, [7; 32], 100), StartRecord::New);

	let states = [&fresh, &paid, &started]
		.map(|session| (session.progress().money_moved, session.money_moved()));

	assert_eq!(states, [(false, false), (true, true), (true, true)]);
}

// A bank payment is approved once it can no longer be reversed, a card
// payment has no such step, and a step the provider skipped takes the
// time of the first later one, so the bar never shows a gap behind it.
#[test]
fn progress_follows_the_rail_and_fills_skipped_steps() {
	let mut bank = chosen(FundingDirection::In, FundingRail::Bank);
	report_at(
		&mut bank,
		FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: false,
			mismatch: None,
		}),
		NOW + 1,
	);
	report_at(
		&mut bank,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 }),
		NOW + 3,
	);
	let mut card = chosen(FundingDirection::In, FundingRail::Card);
	report_at(&mut card, FundingUpdate::In(FundingInboundUpdate::Converting), NOW + 2);
	let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
	report_at(
		&mut out,
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [2; 32], amount: 100 }),
		NOW + 1,
	);
	assert!(out.settle(100, NOW + 4));

	assert_eq!(
		(steps(&bank.progress()), steps(&card.progress()), steps(&out.progress())),
		(
			vec![
				(FundingStep::Started, Some(NOW)),
				(FundingStep::Payment, Some(NOW + 1)),
				(FundingStep::Approved, Some(NOW + 3)),
				(FundingStep::Conversion, Some(NOW + 3)),
				(FundingStep::Added, None),
			],
			vec![
				(FundingStep::Started, Some(NOW)),
				(FundingStep::Payment, Some(NOW + 2)),
				(FundingStep::Conversion, Some(NOW + 2)),
				(FundingStep::Added, None),
			],
			vec![
				(FundingStep::Started, Some(NOW)),
				(FundingStep::Payment, Some(NOW + 1)),
				(FundingStep::Sent, Some(NOW + 4)),
			],
		)
	);
}

// The provider's references can come at any point, even while funds
// move, without changing which step comes next or what settles the
// session; the latest of each is shown.
#[test]
fn details_sit_outside_the_order_of_steps() {
	let mut session = chosen(FundingDirection::In, FundingRail::Bank);
	let crediting =
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 });
	reported(
		&mut session,
		&[
			FundingUpdate::Details {
				transaction_id: Some("tx-1".into()),
				reference: Some("REF".into()),
			},
			crediting.clone(),
			FundingUpdate::Details { transaction_id: Some("tx-2".into()), reference: None },
			FundingUpdate::In(FundingInboundUpdate::Delivered),
		],
	);
	let progress = session.progress();

	assert_eq!(
		(
			session.assignment().last_update,
			session.settlement_due(),
			progress.transaction_id,
			progress.reference,
		),
		(
			Some(FundingUpdate::In(FundingInboundUpdate::Delivered)),
			Some(Settlement::TopUps(vec![named([1; 32], 100)])),
			Some("tx-2".to_string()),
			Some("REF".to_string()),
		)
	);
}

// The RFC lets a provider send its references at any point while the
// session is open, in either direction: before any step, once funds move,
// and after it is done but before the core settled the session. Once the
// session ended, nothing more is taken.
#[test]
fn details_are_taken_while_the_session_is_open() {
	let details = || FundingUpdate::Details { transaction_id: Some("tx".into()), reference: None };
	let mut inbound = served(FundingDirection::In);
	let fresh = report(&mut inbound, details());
	reported(
		&mut inbound,
		&[
			FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 }),
			FundingUpdate::In(FundingInboundUpdate::Delivered),
		],
	);
	let done = report(&mut inbound, details());
	let mut outbound = served(FundingDirection::Out);
	reported(
		&mut outbound,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [2; 32],
			amount: 100,
		})],
	);
	let collecting = report(&mut outbound, details());
	assert!(outbound.settle(100, NOW));
	let ended = report(&mut outbound, details());

	assert_eq!(
		(fresh, done, collecting, ended),
		(Ok(()), Ok(()), Ok(()), Err(HostFundingReportError::NotFound))
	);
}

// `Details` and `Deposit` sit outside the order of the others, so a
// session keeps a bounded number of each, and their texts are short plain
// lines: no line breaks, control or invisible characters.
#[test]
fn details_and_deposits_are_bounded_and_plain() {
	let details =
		|text: &str| FundingUpdate::Details { transaction_id: Some(text.into()), reference: None };
	let deposit = |deposit| FundingUpdate::In(FundingInboundUpdate::Deposit { deposit });
	let mut session = served(FundingDirection::In);
	let kept: Vec<_> = (0..DETAILS_LIMIT).map(|_| report(&mut session, details("tx"))).collect();
	let past_the_limit = report(&mut session, details("tx"));
	let mut session = served(FundingDirection::In);
	let deposits: Vec<_> = (0..DEPOSITS_LIMIT)
		.map(|_| report(&mut session, deposit(usdt_deposit(1))))
		.collect();
	let deposits_past_the_limit = report(&mut session, deposit(usdt_deposit(1)));
	let mut session = served(FundingDirection::In);
	let refused = [
		report(&mut session, FundingUpdate::Details { transaction_id: None, reference: None }),
		report(&mut session, details("two\nlines")),
		report(&mut session, details("\u{202E}gnp.exe")),
		report(&mut session, details(&"x".repeat(FUNDING_DETAIL_MAX_CHARS + 1))),
		report(
			&mut session,
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
				finalized: false,
				mismatch: Some(FundingReceived { asset: "US\u{0}DT".into(), amount: 1 }),
			}),
		),
	];

	assert_eq!(
		(
			kept.iter().all(Result::is_ok),
			past_the_limit,
			deposits.iter().all(Result::is_ok),
			deposits_past_the_limit,
			refused,
		),
		(
			true,
			Err(HostFundingReportError::InvalidDetail),
			true,
			Err(HostFundingReportError::InvalidDetail),
			[(); 5].map(|()| Err(HostFundingReportError::InvalidDetail)),
		)
	);
}

// Deposit instructions name something to pay, have not expired, carry a
// payment URI rather than a page or a script, and pay by the rail and
// asset of the quote the user chose.
#[test]
fn deposit_instructions_must_fit_the_chosen_quote() {
	let usdt = |amount, decimals, uri: Option<&str>, expires_at| FundingDeposit::Crypto {
		address: "0xdeposit".to_string(),
		network: "Ethereum".to_string(),
		asset: "USDT".to_string(),
		amount,
		decimals,
		exact: true,
		uri: uri.map(str::to_string),
		expires_at,
	};
	let chosen = |rail, asset: &str| {
		Some(FundingChoice {
			quote: FundingQuote {
				quote_id: "q1".into(),
				send_amount: 100,
				receive_amount: 100,
				provider_fee: 0,
				network_fee: 0,
				eta_secs: None,
				expires_at: None,
			},
			rail,
			asset: asset.into(),
		})
	};
	let fits = |deposit: FundingDeposit, choice: Option<FundingChoice>| {
		let mut session = served(FundingDirection::In);
		session.choice = choice;
		report(&mut session, FundingUpdate::In(FundingInboundUpdate::Deposit { deposit })).is_ok()
	};
	let later = Some(NOW + 1_000);

	assert_eq!(
		[
			fits(usdt(100, 6, None, later), chosen(FundingRail::Crypto, "usdt")),
			fits(usdt(100, 6, Some("ethereum:0xdeposit@1/transfer?uint256=100"), later), None),
			fits(usdt(0, 6, None, later), None),
			fits(usdt(100, 6, Some("javascript:alert(1)"), later), None),
			fits(usdt(100, 6, Some("https://phish.example/pay"), later), None),
			fits(usdt(100, 6, None, Some(NOW)), None),
			fits(usdt(100, DECIMALS_LIMIT + 1, None, later), None),
			fits(usdt(100, 6, None, later), chosen(FundingRail::Crypto, "USDC")),
			fits(usdt(100, 6, None, later), chosen(FundingRail::Card, "EUR")),
		],
		[true, true, false, false, false, false, false, false, false]
	);
}

fn usdt_deposit(amount: u128) -> FundingDeposit {
	FundingDeposit::Crypto {
		address: "0xdeposit".to_string(),
		network: "Ethereum".to_string(),
		asset: "USDT".to_string(),
		amount,
		decimals: 6,
		exact: true,
		uri: None,
		expires_at: Some(NOW + 1_000),
	}
}

// A short payment, as getcash handles it: the provider says what arrived
// and asks for the rest at the same address; the host shows the latest
// instructions. Once funds move, or for a withdrawal, there is nothing
// left for the user to pay.
#[test]
fn a_short_payment_asks_for_the_rest_until_funds_move() {
	let mut session = served(FundingDirection::In);
	let short = FundingReceived { asset: "USDT".to_string(), amount: 60 };
	reported(
		&mut session,
		&[
			FundingUpdate::In(FundingInboundUpdate::Deposit { deposit: usdt_deposit(100) }),
			FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
				finalized: false,
				mismatch: Some(short.clone()),
			}),
			FundingUpdate::In(FundingInboundUpdate::Deposit { deposit: usdt_deposit(40) }),
		],
	);
	let progress = session.progress();
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 })],
	);
	let after_funds_move = report(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::Deposit { deposit: usdt_deposit(1) }),
	);
	let withdrawal = report(
		&mut served(FundingDirection::Out),
		FundingUpdate::In(FundingInboundUpdate::Deposit { deposit: usdt_deposit(1) }),
	);

	assert_eq!(
		(progress.deposit, progress.mismatch, after_funds_move, withdrawal),
		(
			Some(usdt_deposit(40)),
			Some(short),
			Err(HostFundingReportError::OutOfOrder),
			Err(HostFundingReportError::OutOfOrder),
		)
	);
}

// A repeat under one id must ask what the first call asked, the host's
// refusal drops only funds it never took, and a session holds a bounded
// number of started funds.
#[test]
fn started_funds_are_recorded_once_per_id_and_bounded() {
	let mut session = served(FundingDirection::In);
	let first = start(&mut session, [1; 32], 70);
	let repeat = start(&mut session, [1; 32], 70);
	let other_amount = start(&mut session, [1; 32], 71);
	let other_terms = session.record_start([1; 32], 70, [5; 32], NOW);
	taken(&mut session, [2; 32], 10);
	start(&mut session, [3; 32], 10);
	let full = start(&mut session, [4; 32], 10);
	let forgot_taken = session.forget_start(&[2; 32]);
	let forgot_pending = session.forget_start(&[3; 32]);

	assert_eq!(
		(
			[first, repeat, other_amount, other_terms, full],
			forgot_taken,
			forgot_pending,
			session.pending_starts().map(|funds| funds.host_id).collect::<Vec<_>>(),
		),
		(
			[
				StartRecord::New,
				StartRecord::Repeat,
				StartRecord::Conflict,
				StartRecord::Conflict,
				StartRecord::Full,
			],
			false,
			true,
			vec![[1; 32]],
		)
	);
}

// A partial claim is retried with a new top-up, as getcash does in up to
// three attempts; the host follows all of them once the provider is done.
#[test]
fn an_inbound_session_settles_from_every_top_up_it_names() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[
			FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 70 }),
			FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [2; 32], amount: 30 }),
		],
	);
	let repeated = report(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 30 }),
	);
	let due_before_done = session.settlement_due();
	reported(&mut session, &[FundingUpdate::In(FundingInboundUpdate::Delivered)]);

	assert_eq!(
		(repeated, due_before_done, session.settlement_due()),
		(
			Err(HostFundingReportError::OutOfOrder),
			None,
			Some(Settlement::TopUps(vec![named([1; 32], 70), named([2; 32], 30)]))
		)
	);
}

// Once a top-up or payment started, the money is in motion: the provider
// cannot fail the session and no deadline ends it.
#[test]
fn moving_funds_are_settled_by_the_host_not_the_provider_or_the_clock() {
	let mut session = served(FundingDirection::Out);
	reported(
		&mut session,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 40,
		})],
	);

	let failed =
		report(&mut session, FundingUpdate::Failed { reason: FundingFailure::ProviderTimeout });
	let expired = session.expire_if_due(u64::MAX);

	assert_eq!(
		(failed, expired, session.expires_at(), session.settlement_due()),
		(
			Err(HostFundingReportError::OutOfOrder),
			false,
			None,
			Some(Settlement::Payment(named([3; 32], 40)))
		)
	);
}

#[test]
fn settling_records_what_moved_and_nothing_fails_the_session() {
	let mut credited = served(FundingDirection::In);
	let mut debited = served(FundingDirection::Out);
	let mut unclaimed = served(FundingDirection::In);

	credited.settle(90, NOW);
	debited.settle(40, NOW);
	unclaimed.settle(0, NOW);

	assert_eq!(
		(credited.wire_item(), debited.wire_item(), unclaimed.wire_item()),
		(
			HostFundingStatusSubscribeItem::Delivered { credited: 90 },
			HostFundingStatusSubscribeItem::Released { debited: 40 },
			HostFundingStatusSubscribeItem::Failed {
				reason: FundingFailure::Other {
					code: "not_claimed".to_string(),
					message: "Nothing reached your balance".to_string(),
				},
			},
		)
	);
}

#[test]
fn a_provider_failure_ends_the_session_and_is_kept() {
	let mut session = served(FundingDirection::In);
	let failure = FundingUpdate::Failed { reason: FundingFailure::VerificationRefused };

	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::AwaitingPayment), failure.clone()],
	);

	assert_eq!(
		(session.wire_item(), session.last_update()),
		(
			HostFundingStatusSubscribeItem::Failed { reason: FundingFailure::VerificationRefused },
			Some(&failure)
		)
	);
}

// A compacted session answers with the same terminal item it had in full.
#[test]
fn a_summary_reads_as_the_session_ended() {
	let delivered = FundingSession {
		stage: FundingStage::Delivered { credited: 90, settled_at_ms: NOW + 1 },
		..session(FundingDirection::In)
	};
	let summary = FundingSessionSummary::from(delivered.clone());

	assert_eq!(
		(summary.wire_item(), summary.settled_at_ms()),
		(delivered.wire_item(), Some(NOW + 1))
	);
}
