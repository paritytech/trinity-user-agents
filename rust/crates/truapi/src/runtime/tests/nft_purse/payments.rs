//! Listening for payments through the product-facing trait.

use futures::{StreamExt, executor::block_on};
use truapi::versioned::nft_purse::{
	HostNftPurseListenOneTimePaymentError, HostNftPurseListenOneTimePaymentItem,
};

use super::{cheques::create_receive_key, operations::product_purses, *};
use crate::test_support::{statement_page_frame, subscribe_ack_frame};

type OneTimePayments = Subscription<
	HostNftPurseListenOneTimePaymentItem,
	CallError<HostNftPurseListenOneTimePaymentError>,
>;

fn listen_one_time(host: &ProductRuntimeHost, key: v01::NftPurseReceiveKey) -> OneTimePayments {
	let request = v01::HostNftPurseListenOneTimePaymentRequest { key };
	let request = HostNftPurseListenOneTimePaymentRequest::V1(request);
	block_on(host.listen_one_time_payment(&CallContext::default(), request))
}

/// The JSON-RPC methods `platform` was sent, in order.
fn sent_methods(platform: &StubPlatform) -> Vec<String> {
	let sent = platform.sent_rpc.lock().unwrap();
	let method = |request: &String| {
		let request: serde_json::Value = serde_json::from_str(request).unwrap();
		request["method"].as_str().unwrap_or_default().to_owned()
	};
	sent.iter().map(method).collect()
}

/// The topic of the first statement subscription `platform` was sent.
fn subscribed_topic(platform: &StubPlatform) -> v01::StatementTopic {
	let sent = platform.sent_rpc.lock().unwrap();
	let request: serde_json::Value = serde_json::from_str(&sent[0]).unwrap();
	let topic = request["params"][0]["matchAll"][0].as_str().unwrap();
	let topic = hex::decode(topic.trim_start_matches("0x")).unwrap();
	v01::StatementTopic(topic.try_into().unwrap())
}

/// A platform whose statement subscription is acknowledged, with nothing
/// stored under its topic.
fn platform_with_empty_topic() -> Arc<StubPlatform> {
	Arc::new(StubPlatform {
		rpc_responses: vec![
			subscribe_ack_frame("truapi:1", "payments"),
			statement_page_frame("payments", vec![], Some(0)),
		],
		..Default::default()
	})
}

/// The product shares the channel with the sender, so it comes first, and it
/// is the topic the host watches.
#[test]
fn a_one_time_listen_emits_the_watched_channel_first() {
	let platform = platform_with_empty_topic();
	let purses = product_purses(platform.clone());
	let key = create_receive_key(&purses.host, TICKETS).unwrap();

	let first = block_on(listen_one_time(&purses.host, key).next());

	let sss_topic = subscribed_topic(&platform);
	let channel = v01::NftPursePaymentTransmissionChannel::Standard { sss_topic };
	let expected = HostNftPurseListenOneTimePaymentItem::V1(
		v01::HostNftPurseListenOneTimePaymentItem::Channel(channel),
	);
	assert_eq!(first, Some(Ok(expected)));
}

/// A receive key this host never made reveals nothing and opens no
/// subscription.
#[test]
fn a_one_time_listen_on_an_unknown_key_is_refused() {
	let platform = platform_with_empty_topic();
	let purses = product_purses(platform.clone());

	let first = block_on(listen_one_time(&purses.host, v01::NftPurseReceiveKey([1; 32])).next());

	let refused = HostNftPurseListenOneTimePaymentError::V1(
		v01::NftPurseListenOneTimePaymentError::ReceiveKeyNotFound,
	);
	assert_eq!(first, Some(Err(CallError::Domain(refused))));
	assert!(sent_methods(&platform).is_empty());
}

#[test]
fn a_one_time_listen_needs_a_paired_session() {
	let (_dir, host) = nft_host(platform_with_empty_topic(), false);

	let first = block_on(listen_one_time(&host, v01::NftPurseReceiveKey([1; 32])).next());

	assert_eq!(first, Some(Err(CallError::Denied)));
}

/// A listen that never watched must not hand out a channel nobody watches.
#[test]
fn a_one_time_listen_that_cannot_connect_emits_no_channel() {
	let platform = Arc::new(StubPlatform {
		chain_connect_error: Some("the People chain is unreachable"),
		..Default::default()
	});
	let purses = product_purses(platform);
	let key = create_receive_key(&purses.host, TICKETS).unwrap();

	let mut payments = listen_one_time(&purses.host, key);

	let first = block_on(payments.next());
	assert!(matches!(
		first,
		Some(Err(CallError::Domain(HostNftPurseListenOneTimePaymentError::V1(
			v01::NftPurseListenOneTimePaymentError::Other(_)
		))))
	));
	assert_eq!(block_on(payments.next()), None);
}

/// No subscription outlives the call that opened it.
#[test]
fn dropping_a_one_time_listen_unsubscribes() {
	let platform = platform_with_empty_topic();
	let purses = product_purses(platform.clone());
	let key = create_receive_key(&purses.host, TICKETS).unwrap();
	let mut payments = listen_one_time(&purses.host, key);
	block_on(payments.next());

	drop(payments);

	let methods = sent_methods(&platform);
	assert_eq!(methods.last().map(String::as_str), Some("statement_unsubscribeStatement"));
}
