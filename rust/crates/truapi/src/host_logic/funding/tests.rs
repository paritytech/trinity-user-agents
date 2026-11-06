use super::*;

const NOW: u64 = 1_700_000_000_000;
const PROVIDER: &str = "ramp.dot";

fn provider() -> ProductId {
	ProductId::for_test(PROVIDER)
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

// An ended session is compacted at once, acknowledged or not, keeping
// whether the host told the user. What can still change stays in full: a
// session in flight, and a cancelled top-up a late payment may open.
#[test]
fn ended_sessions_are_compacted_at_once_unless_they_can_still_change() {
	let seen = FundingSession { acknowledged: true, ..expired("fs_seen", NOW) };
	let unseen = expired("fs_unseen", NOW + 1);
	let cancelled = FundingSession {
		intent: "fs_cancelled".to_string(),
		provider_id: Some(provider()),
		stage: FundingStage::Failed { reason: FundingFailure::Cancelled, settled_at_ms: NOW },
		..session(FundingDirection::In)
	};
	let open = session(FundingDirection::Out);

	let (kept, ended) =
		retained([seen.clone(), unseen.clone(), cancelled.clone(), open.clone()], NOW + 2);
	let (later, _) = retained([cancelled], NOW + LATE_ARRIVAL_MS + 1);

	assert_eq!((kept.len(), ended.len(), later.len()), (2, 2, 0));
	assert_eq!(
		ended
			.iter()
			.map(|session| (session.intent.as_str(), session.acknowledged))
			.collect::<Vec<_>>(),
		vec![("fs_seen", true), ("fs_unseen", false)]
	);
}

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
	let direction = session.direction;
	session.record_start(direction, host_id, amount, [0; 32], NOW)
}

/// Record funds the provider started and the host took.
fn taken(session: &mut FundingSession, host_id: [u8; 32], amount: u128) {
	start(session, host_id, amount);
	session.accept_start(&host_id);
}

/// Record a top-up the provider started to refund a withdrawal, and the
/// host took.
fn refund_taken(session: &mut FundingSession, host_id: [u8; 32], amount: u128) {
	session.record_start(FundingDirection::In, host_id, amount, [0; 32], NOW);
	session.accept_start(&host_id);
}

fn named(host_id: [u8; 32], amount: u128) -> StartedFunds {
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

// A payout already in flight can land after the user accepted a cancel.
// The payout wins: the session ends paid out instead of waiting for a
// refund the provider can no longer make.
#[test]
fn a_payout_landing_after_an_accepted_cancel_ends_the_withdrawal() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	released.cancel(NOW + 1);
	assert!(released.answer_cancel(offer(5, 95, None), NOW + 2));
	released.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 2);
	let mut failed = released.clone();

	let landed = released.report(
		&provider(),
		FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut }),
		|id| *id,
		NOW + 3,
	);
	let failed_payout = failed.report(
		&provider(),
		FundingUpdate::Out(FundingOutboundUpdate::Payout {
			outcome: FundingPayout::Failed { reason: "bank".into() },
		}),
		|id| *id,
		NOW + 3,
	);

	let stored = FundingSession::decode(&mut &released.encode()[..]).expect("decodes");

	assert_eq!(
		(
			landed,
			released.cancel.clone(),
			released.status(NOW + 3),
			stored.status(NOW + 3),
			released.awaits_provider(),
			failed_payout,
			failed.awaits_refund(),
		),
		(
			Ok(()),
			Some(FundingCancel::Overtaken { fee: 5, refund: 95 }),
			FundingStatus::PaidOutAfterCancel,
			FundingStatus::PaidOutAfterCancel,
			false,
			Ok(()),
			true
		)
	);
}

// A withdrawal refunded after an accepted cancel stays in full until its
// payout outcome is in, so a payout that lands anyway is still taken.
#[test]
fn a_refunded_withdrawal_stays_in_full_until_its_payout_outcome() {
	let mut released = served(FundingDirection::Out);
	released.stage = FundingStage::Released { debited: 100, settled_at_ms: NOW };
	start(&mut released, [4; 32], 95);
	released.refund = Some(FundingRefund {
		top_up: released
			.started
			.iter()
			.find(|record| record.host_id == [4; 32])
			.cloned()
			.expect("recorded"),
		refunded: Some(95),
	});
	let mut paid = released.clone();
	paid.updates.push(FundingUpdateRecord {
		update: FundingUpdate::Out(FundingOutboundUpdate::Payout {
			outcome: FundingPayout::PaidOut,
		}),
		at_ms: NOW,
	});

	assert_eq!(
		(
			released.awaits_provider(),
			retained([released], NOW).0.len(),
			retained([paid], NOW).0.len(),
		),
		(false, 1, 0)
	);
}

#[test]
fn a_cancel_after_the_payout_overtook_it_is_too_late() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	released.cancel = Some(FundingCancel::Overtaken { fee: 5, refund: 95 });

	assert_eq!(released.pending_cancel_call(), None);
}

// A refund after a failed payout is owed in full, so the user cannot be
// steered into a fee by cancelling then.
#[test]
fn no_cancel_once_a_refund_is_owed() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	released
		.report(
			&provider(),
			FundingUpdate::Out(FundingOutboundUpdate::Payout {
				outcome: FundingPayout::Failed { reason: "bank".into() },
			}),
			|id| *id,
			NOW + 2,
		)
		.expect("failed payout");

	assert_eq!(released.cancel(NOW + 3), CancelOutcome::Refused);
}

// A released withdrawal stays with its provider until it pays out, so
// the user can still cancel it: on terms the user accepts, the provider
// refunds the debit less its fee instead of paying out, and the session
// ends cancelled with what came back.
#[test]
fn an_accepted_cancel_of_a_released_withdrawal_is_refunded() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	let asked = released.cancel(NOW + 1);
	assert!(released.answer_cancel(offer(5, 95, None), NOW + 2));
	let offered = released.status(NOW + 2);
	let answer = released.confirm_cancel(Some(CancelTerms { fee: 5, refund: 95 }), NOW + 2);
	let cancelling = released.status(NOW + 2);
	let paid_out = released.clone().report(
		&provider(),
		FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut }),
		|id| *id,
		NOW + 3,
	);
	refund_taken(&mut released, [6; 32], 90);
	let off_terms = released.clone().report(
		&provider(),
		FundingUpdate::Out(FundingOutboundUpdate::Refunding { top_up_id: [6; 32], amount: 90 }),
		|id| *id,
		NOW + 3,
	);
	refund_taken(&mut released, [5; 32], 95);
	let refunding =
		FundingUpdate::Out(FundingOutboundUpdate::Refunding { top_up_id: [5; 32], amount: 95 });
	let named = released.report(&provider(), refunding, |id| *id, NOW + 3);
	let settled = released.settle_refund(95);

	assert_eq!(
		(
			asked,
			offered,
			answer,
			cancelling,
			paid_out,
			off_terms,
			named,
			settled,
			released.status(NOW + 4),
			released.awaits_provider(),
		),
		(
			CancelOutcome::Requested,
			FundingStatus::CancelOffered { fee: 5, refund: 95 },
			CancelConfirmation::Accepted,
			FundingStatus::Cancelling,
			Ok(()),
			Err(HostFundingReportError::OutOfOrder),
			Ok(()),
			true,
			FundingStatus::Cancelled { refunded: Some(95) },
			false,
		)
	);
}

// The host words the user's cancel from the status while it is open: a
// request, an offer to answer, and a refusal until the provider reports
// its next step, after which the session reads as before.
#[test]
fn the_status_follows_the_users_cancel() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		})],
	);
	cancelled(&mut session);
	let requested = session.status(NOW);
	session.answer_cancel(refused("settled"), NOW);
	let refused = session.status(NOW);
	reported(&mut session, &[FundingUpdate::In(FundingInboundUpdate::Converting)]);
	let carried_on = session.status(NOW);
	cancelled(&mut session);
	session.answer_cancel(offer(2, 98, None), NOW);
	let offered = session.status(NOW);
	session.confirm_cancel(None, NOW);

	assert_eq!(
		(requested, refused, carried_on, offered, session.status(NOW)),
		(
			FundingStatus::CancelRequested,
			FundingStatus::CancelRefused { reason: "settled".into() },
			FundingStatus::ConvertingToCash,
			FundingStatus::CancelOffered { fee: 2, refund: 98 },
			FundingStatus::ConvertingToCash,
		)
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

// The CASH card opens a session with no amount and the user enters it in
// the overlay, so the provider must be told the amount of the quote the
// user chose; a product's own amount stands.
#[test]
fn the_chosen_quote_sets_an_amount_the_session_lacks() {
	let quote = FundingQuote {
		quote_id: "q1".to_string(),
		send_amount: 5_100,
		receive_amount: 5_000,
		provider_fee: 100,
		network_fee: 0,
		eta_secs: None,
		expires_at: None,
		asset_decimals: 2,
	};
	let choice =
		FundingChoice { quote, rail: FundingRail::Card, asset: "EUR".to_string(), network: None };
	let mut open = FundingSession::new("fs_1".to_string(), None, FundingDirection::In, None, NOW);
	let mut asked = session(FundingDirection::In);

	assert!(open.assign(&provider(), choice.clone(), NOW));
	assert!(asked.assign(&provider(), choice, NOW));

	assert_eq!((open.assignment().amount, asked.assignment().amount), (Some(5_000), Some(100)));
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
		asset_decimals: 2,
	};
	let choice = FundingChoice { quote, rail, asset: "EUR".to_string(), network: None };
	assert!(session.assign(&provider(), choice, NOW));
	session
}

// Each in-flight line follows the rail: a bank transfer waits for the
// transfer, a card for the payment and crypto for the deposit, and all
// convert once the provider has the money. A withdrawal is initiated
// until released, then converts to the asset on crypto or prepares the
// payout on card and bank.
#[test]
fn the_status_follows_the_direction_and_rail() {
	let status = |direction, rail, updates: &[FundingUpdate]| {
		let mut session = chosen(direction, rail);
		for (index, update) in updates.iter().enumerate() {
			report_at(&mut session, update.clone(), NOW + 1 + index as u64);
		}
		session.status(NOW + 10)
	};
	let received = FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
		finalized: false,
		mismatch: None,
	});
	let collecting =
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [2; 32], amount: 100 });
	let released = |rail, asset: &str| {
		let mut session = chosen(FundingDirection::Out, rail);
		if let Some(choice) = session.choice.as_mut() {
			choice.asset = asset.to_string();
		}
		report_at(&mut session, collecting.clone(), NOW + 1);
		assert!(session.settle(100, NOW + 2));
		session.status(NOW + 10)
	};

	assert_eq!(
		[
			status(FundingDirection::In, FundingRail::Bank, &[]),
			status(
				FundingDirection::In,
				FundingRail::Card,
				&[FundingUpdate::In(FundingInboundUpdate::AwaitingPayment)]
			),
			status(FundingDirection::In, FundingRail::Crypto, &[]),
			status(FundingDirection::In, FundingRail::Card, &[received]),
			status(FundingDirection::Out, FundingRail::Bank, std::slice::from_ref(&collecting)),
			released(FundingRail::Bank, "EUR"),
			released(FundingRail::Crypto, "USDT"),
		],
		[
			FundingStatus::WaitingForTransfer,
			FundingStatus::WaitingForPayment,
			FundingStatus::WaitingForDeposit,
			FundingStatus::ConvertingToCash,
			FundingStatus::TransactionInitiated,
			FundingStatus::PreparingPayout,
			FundingStatus::ConvertingTo { asset: "USDT".to_string() },
		]
	);
}

// A provider that holds the payment and stays silent past the time its
// quote gave is taking a little longer, which the host learns to read
// again for; a report starts the clock again.
#[test]
fn a_session_past_its_quoted_time_is_taking_longer() {
	let mut session = chosen(FundingDirection::In, FundingRail::Card);
	if let Some(choice) = session.choice.as_mut() {
		choice.quote.eta_secs = Some(60);
	}
	report_at(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: false,
			mismatch: None,
		}),
		NOW,
	);
	let before = session.progress(NOW + 59_999);
	let after = session.status(NOW + 60_000);
	report_at(&mut session, FundingUpdate::In(FundingInboundUpdate::Converting), NOW + 60_001);

	assert_eq!(
		(before.status, before.longer_at_ms, after, session.status(NOW + 60_002)),
		(
			FundingStatus::ConvertingToCash,
			Some(NOW + 60_000),
			FundingStatus::TakingLonger,
			FundingStatus::ConvertingToCash,
		)
	);
}

// Until the provider holds the payment a top-up waits on the user: a
// transfer or deposit that takes days is not the provider taking longer,
// so the waiting status stays and the host has no time to read again at.
// A withdrawal keeps its clock from the quote, so by then it is taking
// longer (and the moment, already past, is not given again).
#[test]
fn a_top_up_waiting_on_the_users_payment_is_never_taking_longer() {
	let quoted = |direction, rail| {
		let mut session = chosen(direction, rail);
		if let Some(choice) = session.choice.as_mut() {
			choice.quote.eta_secs = Some(60);
		}
		session
	};
	let mut withdrawal = quoted(FundingDirection::Out, FundingRail::Bank);
	report_at(
		&mut withdrawal,
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [2; 32], amount: 100 }),
		NOW,
	);
	let later = NOW + 600_000;
	let waiting = |session: &FundingSession| {
		let progress = session.progress(later);
		(progress.status, progress.longer_at_ms)
	};

	assert_eq!(
		[
			waiting(&quoted(FundingDirection::In, FundingRail::Bank)),
			waiting(&quoted(FundingDirection::In, FundingRail::Crypto)),
			waiting(&quoted(FundingDirection::In, FundingRail::Card)),
			waiting(&withdrawal),
		],
		[
			(FundingStatus::WaitingForTransfer, None),
			(FundingStatus::WaitingForDeposit, None),
			(FundingStatus::WaitingForPayment, None),
			(FundingStatus::TakingLonger, None),
		]
	);
}

// How a session ended is one line too: credited, paid out, the payment
// given back, a failed payout with what its refund returned, or a failure
// with its reason.
#[test]
fn the_status_says_how_the_session_ended() {
	let mut delivered = chosen(FundingDirection::In, FundingRail::Card);
	assert!(delivered.settle(100, NOW + 1));
	let mut refunded = chosen(FundingDirection::In, FundingRail::Card);
	assert!(refunded.fail(FundingFailure::Refunded, NOW + 1));
	let mut expired = chosen(FundingDirection::In, FundingRail::Card);
	assert!(expired.fail(FundingFailure::Expired, NOW + 1));
	let paid = |outcome| {
		let mut session = chosen(FundingDirection::Out, FundingRail::Card);
		report_at(
			&mut session,
			FundingUpdate::Out(FundingOutboundUpdate::Collecting {
				payment_id: [2; 32],
				amount: 100,
			}),
			NOW + 1,
		);
		assert!(session.settle(100, NOW + 2));
		report_at(
			&mut session,
			FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome }),
			NOW + 3,
		);
		session
	};
	let mut payout_failed = paid(FundingPayout::Failed { reason: "closed".to_string() });
	let failed_before_refund = payout_failed.status(NOW + 4);
	report_at(
		&mut payout_failed,
		FundingUpdate::Out(FundingOutboundUpdate::Refunding { top_up_id: [6; 32], amount: 95 }),
		NOW + 4,
	);
	assert!(payout_failed.settle_refund(95));

	assert_eq!(
		[
			delivered.status(NOW + 5),
			paid(FundingPayout::PaidOut).status(NOW + 5),
			refunded.status(NOW + 5),
			failed_before_refund,
			payout_failed.status(NOW + 5),
			expired.status(NOW + 5),
		],
		[
			FundingStatus::ToppedUp,
			FundingStatus::Sent,
			FundingStatus::Refunded,
			FundingStatus::PayoutFailed { refunded: None },
			FundingStatus::PayoutFailed { refunded: Some(95) },
			FundingStatus::Failed { reason: FundingFailure::Expired },
		]
	);
}

fn report_at(session: &mut FundingSession, update: FundingUpdate, at_ms: u64) {
	if let Some((id, amount)) = update.named_funds() {
		taken(session, *id, amount);
	}
	session.report(&provider(), update, |id| *id, at_ms).expect("reported");
}

fn detail(fact: FundingFact, at_ms: u64) -> FundingUpdate {
	FundingUpdate::Progress { detail: FundingDetail { fact, at_ms: Some(at_ms) } }
}

// The line under the status says what the provider is doing within a
// step, so it must not outlive the step, and each check is news that
// the provider is still there.
#[test]
fn a_progress_detail_shows_for_its_step_and_counts_as_hearing_from_the_provider() {
	let mut session = chosen(FundingDirection::In, FundingRail::Crypto);
	report_at(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: false,
			mismatch: None,
		}),
		NOW + 1_000,
	);
	let confirmations = FundingFact::Confirmations { done: 2, required: 3 };
	report_at(&mut session, detail(confirmations.clone(), NOW + 1_500), NOW + 2_000);
	let progress = session.progress(NOW + 3_000);
	assert_eq!(
		progress.detail,
		Some(FundingDetail { fact: confirmations, at_ms: Some(NOW + 1_500) })
	);
	assert_eq!(progress.last_heard_at_ms, Some(NOW + 2_000));
	assert_eq!(session.updates.len(), 1, "a progress detail is not kept in the history");

	report_at(
		&mut session,
		FundingUpdate::Details { transaction_id: Some("tx".into()), reference: None },
		NOW + 4_000,
	);
	assert!(session.progress(NOW + 4_000).detail.is_some(), "a reference does not move the step");

	report_at(
		&mut session,
		FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		}),
		NOW + 5_000,
	);
	let progress = session.progress(NOW + 5_000);
	assert_eq!(progress.detail, None);
	assert_eq!(progress.last_heard_at_ms, Some(NOW + 5_000));
}

#[test]
fn a_progress_detail_that_does_not_fit_the_session_is_refused() {
	let mut card = chosen(FundingDirection::In, FundingRail::Card);
	let mut crypto = chosen(FundingDirection::In, FundingRail::Crypto);
	let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
	let invalid = Err(HostFundingReportError::InvalidDetail);
	let other = |text: &str| detail(FundingFact::Other { text: text.to_string() }, NOW);
	assert_eq!(
		report(&mut card, detail(FundingFact::Confirmations { done: 1, required: 3 }, NOW)),
		invalid
	);
	assert_eq!(
		report(&mut crypto, detail(FundingFact::Confirmations { done: 4, required: 3 }, NOW)),
		invalid
	);
	assert_eq!(
		report(&mut crypto, detail(FundingFact::Confirmations { done: 0, required: 0 }, NOW)),
		invalid
	);
	assert_eq!(report(&mut card, detail(FundingFact::Parts { done: 3, total: 2 }, NOW)), invalid);
	assert_eq!(report(&mut card, detail(FundingFact::Attempt { n: 0, of: None }, NOW)), invalid);
	assert_eq!(report(&mut card, detail(FundingFact::Attempt { n: 3, of: Some(2) }, NOW)), invalid);
	assert_eq!(report(&mut out, detail(FundingFact::SentByBank, NOW)), invalid);
	assert_eq!(report(&mut card, other("")), invalid);
	assert_eq!(report(&mut card, other("two\nlines")), invalid);
	assert_eq!(report(&mut card, other("Pay \u{202E}niaga")), invalid);
	assert_eq!(report(&mut card, other("zero\u{200B}width")), invalid);
	assert_eq!(report(&mut card, other(&"a".repeat(FUNDING_DETAIL_MAX_CHARS + 1))), invalid);
	assert_eq!(report(&mut card, other(&"€".repeat(FUNDING_DETAIL_MAX_CHARS))), Ok(()));
	assert_eq!(
		report(&mut card, other(&"𝄞".repeat(FUNDING_DETAIL_MAX_CHARS))),
		Ok(()),
		"320 bytes"
	);
	assert_eq!(
		report(&mut crypto, detail(FundingFact::Confirmations { done: 3, required: 3 }, NOW)),
		Ok(())
	);
	assert_eq!(report(&mut out, detail(FundingFact::Attempt { n: 2, of: Some(3) }, NOW)), Ok(()));
	assert!(card.updates.is_empty() && crypto.updates.is_empty() && out.updates.is_empty());
}

#[test]
fn a_progress_detail_is_not_stored_and_not_taken_once_the_session_ended() {
	let mut session = served(FundingDirection::In);
	assert_eq!(report(&mut session, detail(FundingFact::Checked, NOW)), Ok(()));
	let stored = FundingSession::decode(&mut &session.encode()[..]).expect("decodes");
	assert_eq!(stored.detail, None);
	reported(&mut session, &[FundingUpdate::Failed { reason: FundingFailure::Expired }]);
	assert_eq!(
		report(&mut session, detail(FundingFact::Checked, NOW)),
		Err(HostFundingReportError::NotFound)
	);
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
		.map(|session| (session.progress(NOW).money_moved, session.money_moved()));

	assert_eq!(states, [(false, false), (true, true), (true, true)]);
}

// The timeline has three milestones in each direction: a top-up is
// received once the provider sees the payment and added once credited, a
// withdrawal is received by the provider once released and sent once the
// payout lands. A milestone the provider skipped takes the time of the
// first later one, so the timeline never shows a gap behind it.
#[test]
fn progress_has_three_milestones_and_fills_skipped_ones() {
	let mut card = chosen(FundingDirection::In, FundingRail::Card);
	report_at(
		&mut card,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 }),
		NOW + 3,
	);
	assert!(card.settle(100, NOW + 5));
	let mut bank = chosen(FundingDirection::In, FundingRail::Bank);
	report_at(
		&mut bank,
		FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: false,
			mismatch: None,
		}),
		NOW + 2,
	);
	let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
	report_at(
		&mut out,
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [2; 32], amount: 100 }),
		NOW + 1,
	);
	assert!(out.settle(100, NOW + 4));
	report_at(
		&mut out,
		FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut }),
		NOW + 6,
	);

	assert_eq!(
		(steps(&card.progress(NOW)), steps(&bank.progress(NOW)), steps(&out.progress(NOW))),
		(
			vec![
				(FundingStep::Started, Some(NOW)),
				(FundingStep::Received, Some(NOW + 3)),
				(FundingStep::Added, Some(NOW + 5)),
			],
			vec![
				(FundingStep::Started, Some(NOW)),
				(FundingStep::Received, Some(NOW + 2)),
				(FundingStep::Added, None),
			],
			vec![
				(FundingStep::Started, Some(NOW)),
				(FundingStep::Received, Some(NOW + 4)),
				(FundingStep::Sent, Some(NOW + 6)),
			],
		)
	);
}

// The line under the milestones follows the provider's steps, and says
// when the provider retries or goes quiet.
#[test]
fn the_band_follows_the_providers_steps() {
	let band = |session: &FundingSession| session.progress(NOW + 10).band;
	let fresh = chosen(FundingDirection::In, FundingRail::Bank);
	let mut checking = chosen(FundingDirection::In, FundingRail::Card);
	report_at(
		&mut checking,
		FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: false,
			mismatch: None,
		}),
		NOW + 1,
	);
	let mut on_its_way = checking.clone();
	report_at(&mut on_its_way, FundingUpdate::In(FundingInboundUpdate::Converting), NOW + 2);
	let mut adding = on_its_way.clone();
	taken(&mut adding, [1; 32], 100);
	report_at(
		&mut adding,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 }),
		NOW + 3,
	);
	let mut added = adding.clone();
	assert!(added.settle(100, NOW + 4));
	let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
	let converting = band(&out);
	report_at(
		&mut out,
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [2; 32], amount: 100 }),
		NOW + 1,
	);
	assert!(out.settle(100, NOW + 2));
	let preparing = band(&out);
	report_at(&mut out, FundingUpdate::Out(FundingOutboundUpdate::Converting), NOW + 3);

	assert_eq!(
		[
			band(&fresh),
			band(&checking),
			band(&on_its_way),
			band(&adding),
			band(&added),
			converting,
			preparing,
			band(&out),
		],
		[
			FundingBand::AwaitingPayment,
			FundingBand::CheckingPayment,
			FundingBand::OnItsWay,
			FundingBand::Adding,
			FundingBand::Added,
			FundingBand::Converting,
			FundingBand::PreparingPayout,
			FundingBand::Sending,
		]
	);
}

// A bank transfer the user had already sent reaches the provider after
// they cancelled the top-up. The cancelled session takes it, opens again
// and settles as delivered, and the host is handed it again; after
// LATE_ARRIVAL_MS, or once something else moved, it stays cancelled.
#[test]
fn a_payment_after_a_cancel_reopens_the_top_up() {
	let mut cancelled = chosen(FundingDirection::In, FundingRail::Bank);
	assert_eq!(cancelled.cancel(NOW + 1), CancelOutcome::Requested);
	assert_eq!(cancelled.cancel(NOW + 2), CancelOutcome::Ended);
	cancelled.acknowledged = true;
	let too_late = cancelled.clone();

	let in_time = cancelled.takes_late_arrival(NOW + 3);
	let reported = cancelled.report(
		&provider(),
		FundingUpdate::In(FundingInboundUpdate::PaymentReceived {
			finalized: true,
			mismatch: None,
		}),
		|id| *id,
		NOW + 3,
	);

	assert_eq!(
		(
			in_time,
			reported,
			cancelled.is_terminal(),
			cancelled.acknowledged,
			too_late.takes_late_arrival(NOW + 2 + LATE_ARRIVAL_MS),
		),
		(true, Ok(()), false, false, false)
	);
}

// After a withdrawal is released the provider still converts what it
// collected and then pays the user out; each is reported once and in
// that order, and an inbound session takes neither once it has ended.
#[test]
fn a_released_withdrawal_takes_converting_then_payout_once_each() {
	let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
	report_at(
		&mut out,
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [2; 32], amount: 100 }),
		NOW + 1,
	);
	assert!(out.settle(100, NOW + 2));
	let mut delivered = chosen(FundingDirection::In, FundingRail::Card);
	report_at(
		&mut delivered,
		FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 100 }),
		NOW + 1,
	);
	assert!(delivered.settle(100, NOW + 2));
	let paid_out =
		|| FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut });

	assert_eq!(
		(
			out.report(
				&provider(),
				FundingUpdate::Out(FundingOutboundUpdate::Converting),
				|id| *id,
				NOW + 3
			),
			out.report(
				&provider(),
				FundingUpdate::Out(FundingOutboundUpdate::Converting),
				|id| *id,
				NOW + 4
			),
			out.report(&provider(), paid_out(), |id| *id, NOW + 5),
			out.report(
				&provider(),
				FundingUpdate::Out(FundingOutboundUpdate::Converting),
				|id| *id,
				NOW + 6
			),
			delivered.report(
				&provider(),
				FundingUpdate::Out(FundingOutboundUpdate::Converting),
				|id| *id,
				NOW + 3
			),
		),
		(
			Ok(()),
			Err(HostFundingReportError::OutOfOrder),
			Ok(()),
			Err(HostFundingReportError::OutOfOrder),
			Err(HostFundingReportError::NotFound),
		)
	);
}

// A provider retrying a payout says so as often as it tries again, only
// between the release and the outcome; the session shows it is retrying
// until the provider converts again or reports how the payout went.
#[test]
fn a_released_withdrawal_takes_payout_retries_until_the_outcome() {
	let mut out = chosen(FundingDirection::Out, FundingRail::Bank);
	let retrying = || FundingUpdate::Out(FundingOutboundUpdate::PayoutRetrying);
	let early = out.report(&provider(), retrying(), |id| *id, NOW + 1);
	report_at(
		&mut out,
		FundingUpdate::Out(FundingOutboundUpdate::Collecting { payment_id: [2; 32], amount: 100 }),
		NOW + 1,
	);
	assert!(out.settle(100, NOW + 2));
	let preparing = out.status(NOW + 2);
	let first = out.report(&provider(), retrying(), |id| *id, NOW + 3);
	let after_first = out.status(NOW + 3);
	let again = out.report(&provider(), retrying(), |id| *id, NOW + 4);
	let converting = out.report(
		&provider(),
		FundingUpdate::Out(FundingOutboundUpdate::Converting),
		|id| *id,
		NOW + 5,
	);
	let after_converting = out.status(NOW + 5);
	let after_conversion = out.report(&provider(), retrying(), |id| *id, NOW + 6);
	let retrying_again = out.status(NOW + 6);
	let paid_out = out.report(
		&provider(),
		FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut }),
		|id| *id,
		NOW + 7,
	);
	let late = out.report(&provider(), retrying(), |id| *id, NOW + 8);

	assert_eq!(
		(early, first, again, converting, after_conversion, paid_out, late),
		(
			Err(HostFundingReportError::OutOfOrder),
			Ok(()),
			Ok(()),
			Ok(()),
			Ok(()),
			Ok(()),
			Err(HostFundingReportError::OutOfOrder),
		)
	);
	assert_eq!(
		(preparing, after_first, after_converting, retrying_again, out.status(NOW + 8)),
		(
			FundingStatus::PreparingPayout,
			FundingStatus::RetryingPayout,
			FundingStatus::PreparingPayout,
			FundingStatus::RetryingPayout,
			FundingStatus::Sent,
		)
	);
	assert_eq!(out.expires_at(), None);
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
	let progress = session.progress(NOW);

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
// A released withdrawal awaiting its payout takes the payout's details;
// once the payout is reported, the session is no longer the provider's.
#[test]
fn details_are_taken_while_a_released_withdrawal_awaits_its_payout() {
	let mut session = session(FundingDirection::Out);
	assert!(session.assign(&ProductId::for_test("ramp.dot"), test_choice(), 0));
	session.updates.push(FundingUpdateRecord {
		update: FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [4; 32],
			amount: 100,
		}),
		at_ms: 0,
	});
	assert!(session.settle(100, 0));
	let details =
		|id: &str| FundingUpdate::Details { transaction_id: Some(id.to_string()), reference: None };

	let awaiting =
		session.report(&ProductId::for_test("ramp.dot"), details("payout-1"), |id| *id, 1);
	let paid = session.report(
		&ProductId::for_test("ramp.dot"),
		FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut }),
		|id| *id,
		2,
	);
	let after = session.report(&ProductId::for_test("ramp.dot"), details("payout-2"), |id| *id, 3);

	assert_eq!(
		(awaiting, paid, after, session.transaction_id()),
		(Ok(()), Ok(()), Err(HostFundingReportError::NotFound), Some("payout-1".to_string()))
	);
}

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
	let awaiting_payout = report(&mut outbound, details());

	assert_eq!((fresh, done, collecting, awaiting_payout), (Ok(()), Ok(()), Ok(()), Ok(())));
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
				asset_decimals: 6,
			},
			rail,
			asset: asset.into(),
			network: None,
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
	let progress = session.progress(NOW);
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

// The user's CASH has already left when a payout fails, so the session
// stays Released and the provider's payout outcome is kept beside it,
// once. A payout reported before the release is out of order, and one
// reported after the host recorded the session hands it to the host
// again.
#[test]
fn a_released_session_takes_one_payout_outcome() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	let failed = FundingPayout::Failed { reason: "bank rejected the transfer".to_string() };
	let paid_out =
		|| FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut });
	let mut recorded = released.clone();
	recorded.acknowledged = true;
	let first = released.report(
		&provider(),
		FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: failed.clone() }),
		|id| *id,
		NOW + 2,
	);
	let second = released.report(&provider(), paid_out(), |id| *id, NOW + 3);
	let mut in_flight = served(FundingDirection::Out);
	let too_early = in_flight.report(&provider(), paid_out(), |id| *id, NOW);
	let after_recording = recorded.report(&provider(), paid_out(), |id| *id, NOW + 4);

	assert_eq!(
		(
			first,
			second,
			too_early,
			after_recording,
			recorded.acknowledged,
			released.progress(NOW).payout,
		),
		(
			Ok(()),
			Err(HostFundingReportError::OutOfOrder),
			Err(HostFundingReportError::OutOfOrder),
			Ok(()),
			false,
			Some(failed),
		)
	);
}

// The host may record a withdrawal as soon as it is released, but the
// provider has yet to pay the user out, so the session is served until it
// reports how that went. After that it stays only as a record its owner
// can read.
#[test]
fn a_recorded_withdrawal_is_kept_until_its_payout_is_reported() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	released.acknowledged = true;
	let served_before = retained([released.clone()], NOW + 1)
		.0
		.iter()
		.any(FundingSession::serves_provider);
	released
		.report(
			&provider(),
			FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: FundingPayout::PaidOut }),
			|id| *id,
			NOW + 2,
		)
		.expect("reported");
	released.acknowledged = true;

	let (kept_after, _) = retained([released], NOW + 2);
	assert_eq!((served_before, kept_after.len()), (true, 0));
}

// A failed payout leaves the user's CASH with the provider, so the
// provider gives it back with a top-up it names in `Refunding`, once and
// only after the failure. The session is followed until that top-up's
// claims are final, then handed to the host again with what came back.
#[test]
fn a_failed_payout_is_refunded_through_a_named_top_up() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	let refunding = |amount| {
		FundingUpdate::Out(FundingOutboundUpdate::Refunding { top_up_id: [5; 32], amount })
	};
	let early = released.clone().report(&provider(), refunding(95), |id| *id, NOW + 2);
	let failed = FundingPayout::Failed { reason: "card closed".to_string() };
	released
		.report(
			&provider(),
			FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: failed }),
			|id| *id,
			NOW + 2,
		)
		.expect("reported");
	let unrecorded = released.clone().report(&provider(), refunding(95), |id| *id, NOW + 3);
	refund_taken(&mut released, [5; 32], 95);
	let other_amount = released.clone().report(&provider(), refunding(90), |id| *id, NOW + 3);
	let named = released.report(&provider(), refunding(95), |id| *id, NOW + 3);
	let again = released.clone().report(&provider(), refunding(95), |id| *id, NOW + 4);
	let due = released.settlement_due();
	released.acknowledged = true;
	let settled = released.settle_refund(95);

	assert_eq!(
		(early, unrecorded, other_amount, named, again, due),
		(
			Err(HostFundingReportError::OutOfOrder),
			Err(HostFundingReportError::NotStarted),
			Err(HostFundingReportError::NotStarted),
			Ok(()),
			Err(HostFundingReportError::OutOfOrder),
			Some(Settlement::Refund(StartedFunds {
				direction: FundingDirection::In,
				host_id: [5; 32],
				amount: 95,
				terms: [0; 32],
				recorded_at_ms: NOW,
				accepted: true,
				named_at_ms: Some(NOW + 3),
			})),
		)
	);
	assert_eq!(
		(
			settled,
			released.progress(NOW).refunded,
			released.acknowledged,
			released.awaits_provider(),
			released.settlement_due(),
		),
		(true, Some(95), false, false, None)
	);
}

// Until the provider refunds a failed payout it still owes the user, so
// the session stays served and never expires.
#[test]
fn a_failed_payout_is_served_until_refunded() {
	let mut released = served(FundingDirection::Out);
	reported(
		&mut released,
		&[FundingUpdate::Out(FundingOutboundUpdate::Collecting {
			payment_id: [3; 32],
			amount: 100,
		})],
	);
	assert!(released.settle(100, NOW + 1));
	let failed = FundingPayout::Failed { reason: "card closed".to_string() };
	released
		.report(
			&provider(),
			FundingUpdate::Out(FundingOutboundUpdate::Payout { outcome: failed }),
			|id| *id,
			NOW + 2,
		)
		.expect("reported");
	released.acknowledged = true;

	assert_eq!(
		(
			released.serves_provider(),
			retained([released.clone()], NOW + 2).0.len(),
			released.expires_at()
		),
		(true, 1, None)
	);
}

// A second top-up after a partial claim is the provider retrying, which
// the host shows as "Retrying your payment"; one top-up is not.
#[test]
fn a_second_top_up_reads_as_retrying() {
	let mut session = served(FundingDirection::In);
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [1; 32], amount: 60 })],
	);
	let after_one = session.progress(NOW).retrying;
	reported(
		&mut session,
		&[FundingUpdate::In(FundingInboundUpdate::Crediting { top_up_id: [2; 32], amount: 40 })],
	);

	assert_eq!((after_one, session.progress(NOW).retrying), (false, true));
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
	let other_terms = session.record_start(FundingDirection::In, [1; 32], 70, [5; 32], NOW);
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
			Some(Settlement::Payment(StartedFunds {
				direction: FundingDirection::Out,
				..named([3; 32], 40)
			}))
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
