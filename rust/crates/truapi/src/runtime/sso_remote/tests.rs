use super::*;
use crate::{
	host_internal::sso_messages::{
		ProductSubtreeRequest, ProductSubtreeResponse, RemoteMessage, RemoteMessageData,
		build_outgoing_request_statement, build_signed_session_response_statement,
	},
	host_logic::{
		sso::pairing::{SsoStatementData, encrypt_session_statement_data},
		statement_store::build_signed_session_request_statement,
	},
	test_support::{new_statements_frame, sso_host_and_responder_sessions, sso_session_info},
};
use futures::stream;
use parity_scale_codec::Encode;
use std::time::Duration;

#[test]
fn sso_message_id_uses_short_opaque_nanoids() {
	let first = sso_message_id();
	let second = sso_message_id();

	assert_eq!(first.len(), 8);
	assert_eq!(second.len(), 8);
	assert_ne!(first, "p:1");
	assert_ne!(second, "p:1");
	assert_ne!(first, second);
	assert!(first.bytes().all(is_nanoid_safe_byte));
	assert!(second.bytes().all(is_nanoid_safe_byte));
}

#[test]
fn fresh_statement_expiry_is_strictly_monotonic() {
	let first = fresh_statement_expiry();
	let second = fresh_statement_expiry();

	assert!(second > first);
	let floor = 42_u64 << 32;
	assert_eq!(next_statement_expiry(floor, floor), floor + 1);
	assert_eq!(next_statement_expiry(floor + 7, floor), floor + 8);
}

fn is_nanoid_safe_byte(value: u8) -> bool {
	value.is_ascii_alphanumeric() || value == b'_' || value == b'-'
}

fn pending_wait<'a>(
	session: &'a SsoSessionInfo,
	cancel: &'a CancellationToken,
) -> RemoteResponseWait<'a> {
	RemoteResponseWait {
		own_statements: stream::pending().boxed(),
		peer_statements: stream::pending().boxed(),
		submit: futures::future::pending().boxed(),
		session,
		statement_request_id: "request-1",
		remote_message_id: "request-1",
		cancel,
		disconnect: None,
	}
}

#[test]
fn sso_remote_response_waiter_reports_timeout_cancellation() {
	let session = sso_session_info();
	let cancel = CancellationToken::default();
	cancel.cancel_with_reason(CancellationReason::TimedOut { timeout: Duration::from_millis(1) });
	let err = futures::executor::block_on(wait_for_sso_remote_response(
		pending_wait(session.sso.as_ref().unwrap(), &cancel),
		|_| None::<Result<(), String>>,
	))
	.unwrap_err();

	let SsoRemoteResponseError::Cancelled(err) = err else {
		panic!("expected cancellation error");
	};
	assert_eq!(err.to_string(), "SSO response timed out after 1ms for request-1");
}

#[test]
fn sso_remote_response_waiter_reports_submit_rejections() {
	let session = sso_session_info();
	let err = futures::executor::block_on(wait_for_sso_remote_response(
		RemoteResponseWait {
			submit: futures::future::ready(Err(SsoRemoteResponseError::Failure(
				"SSO statement submit failed: no allowance".to_string(),
			)))
			.boxed(),
			..pending_wait(session.sso.as_ref().unwrap(), &CancellationToken::default())
		},
		|_| None::<Result<(), String>>,
	))
	.unwrap_err();

	assert_eq!(
		err,
		SsoRemoteResponseError::Failure("SSO statement submit failed: no allowance".to_string())
	);
}

#[test]
fn sso_remote_response_waiter_stops_on_local_disconnect_signal() {
	let session = sso_session_info();
	let (tx, rx) = oneshot::channel();
	tx.send(SSO_LOCAL_DISCONNECT_REASON.to_string()).unwrap();
	let err = futures::executor::block_on(wait_for_sso_remote_response(
		RemoteResponseWait {
			disconnect: Some(rx),
			..pending_wait(session.sso.as_ref().unwrap(), &CancellationToken::default())
		},
		|_| None::<Result<(), String>>,
	))
	.unwrap_err();

	assert_eq!(err, SsoRemoteResponseError::LocalDisconnected);
}

#[test]
fn sso_remote_response_waiter_stops_on_call_cancellation() {
	let session = sso_session_info();
	let cancel = CancellationToken::default();
	let wait =
		wait_for_sso_remote_response(pending_wait(session.sso.as_ref().unwrap(), &cancel), |_| {
			None::<Result<(), String>>
		});

	cancel.cancel();
	let err = futures::executor::block_on(wait).unwrap_err();

	let SsoRemoteResponseError::Cancelled(err) = err else {
		panic!("expected cancellation error");
	};
	assert_eq!(err.to_string(), "SSO response wait cancelled by caller for request-1");
}

fn peer_page(statement: Vec<u8>) -> Value {
	let frame: Value =
		serde_json::from_str(&new_statements_frame("sub", vec![statement])).expect("frame is json");
	frame["params"]["result"].clone()
}

fn subtree_response(responding_to: &str) -> RemoteMessage {
	RemoteMessage {
		message_id: "resp-1".to_string(),
		data: RemoteMessageData::V1(v1::RemoteMessage::ProductSubtreeResponse(Response {
			responding_to: responding_to.to_string(),
			payload: Ok([7; 32]),
		})),
	}
}

fn wait_for_subtree(
	host: &SsoSessionInfo,
	pages: Vec<Value>,
) -> Result<Response<ProductSubtreeResponse>, SsoRemoteResponseError> {
	let cancel = CancellationToken::default();
	futures::executor::block_on(wait_for_sso_remote_response(
		RemoteResponseWait {
			own_statements: stream::empty().boxed(),
			peer_statements: stream::iter(pages.into_iter().map(Ok)).boxed(),
			submit: futures::future::ready(Ok(())).boxed(),
			..pending_wait(host, &cancel)
		},
		reply_matcher::<ProductSubtreeRequest>("request-1"),
	))
}

/// A peer that answers with another response kind must fail the call at
/// once; skipping it would leave the product waiting for the timeout.
#[test]
fn a_reply_of_the_wrong_kind_fails_instead_of_waiting() {
	let (host, responder) = sso_host_and_responder_sessions();
	let wrong_kind = RemoteMessage {
		message_id: "resp-1".to_string(),
		data: RemoteMessageData::V1(v1::RemoteMessage::SignResponse(Response {
			responding_to: "request-1".to_string(),
			payload: Err("nope".to_string()),
		})),
	};
	let statement = build_outgoing_request_statement(
		&responder,
		"resp-statement".to_string(),
		vec![wrong_kind],
		fresh_statement_expiry(),
	)
	.unwrap();

	let err = wait_for_subtree(&host, vec![peer_page(statement)]).unwrap_err();

	assert_eq!(
		err,
		SsoRemoteResponseError::Failure(
			"Unexpected SSO response for product_subtree: SignResponse".to_string()
		)
	);
}

/// Messages are read in wire order: a reply that arrives before the peer's
/// `Disconnected` in the same statement is still delivered.
#[test]
fn a_matching_reply_earlier_in_a_batch_wins_over_a_later_disconnect() {
	let (host, responder) = sso_host_and_responder_sessions();
	let ack = build_signed_session_response_statement(
		&responder,
		"request-1".to_string(),
		0,
		fresh_statement_expiry(),
	)
	.unwrap();
	let disconnect = RemoteMessage {
		message_id: "bye".to_string(),
		data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
	};
	let statement = build_outgoing_request_statement(
		&responder,
		"resp-statement".to_string(),
		vec![subtree_response("request-1"), disconnect],
		fresh_statement_expiry(),
	)
	.unwrap();

	let response = wait_for_subtree(&host, vec![peer_page(ack), peer_page(statement)]).unwrap();

	assert_eq!(response, Response { responding_to: "request-1".to_string(), payload: Ok([7; 32]) });
}

/// A message that does not decode, such as a stale reply from an older peer
/// left on the session channel, is skipped wherever it sits, so it cannot
/// fail a wait for another request.
#[test]
fn an_undecodable_message_is_skipped() {
	let (host, responder) = sso_host_and_responder_sessions();
	let ack = build_signed_session_response_statement(
		&responder,
		"request-1".to_string(),
		0,
		fresh_statement_expiry(),
	)
	.unwrap();
	let statement_with = |data: Vec<Vec<u8>>| {
		let encrypted = encrypt_session_statement_data(
			&responder,
			&SsoStatementData::Request { request_id: "resp-statement".to_string(), data },
		)
		.unwrap();
		build_signed_session_request_statement(&responder, encrypted, fresh_statement_expiry())
			.unwrap()
	};
	let garbage = vec![0xff, 0xff, 0xff];
	let reply = subtree_response("request-1").encode();

	let after = wait_for_subtree(
		&host,
		vec![
			peer_page(ack.clone()),
			peer_page(statement_with(vec![reply.clone(), garbage.clone()])),
		],
	);
	let before = wait_for_subtree(
		&host,
		vec![peer_page(ack), peer_page(statement_with(vec![garbage, reply]))],
	);

	let expected = Ok(Response { responding_to: "request-1".to_string(), payload: Ok([7; 32]) });
	assert_eq!((before, after), (expected.clone(), expected));
}
